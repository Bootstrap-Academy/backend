"""Synthetic PostgreSQL relay acceptance. Only the explicitly owned L2 database.
No sockets except loopback PostgreSQL; no email or external recipients.
"""
import asyncio, json, os
from uuid import uuid4
import asyncpg

DSN=os.environ.get('L2_DATABASE_URL','postgresql://l2test@127.0.0.1:55900/l2backend')
assert '@127.0.0.1:' in DSN and DSN.endswith('/l2backend')
async def op(db,name,body=None):
 return json.loads(await db.fetchval('SELECT backend_moderation($1,NULL,$2::jsonb)::text',name,json.dumps(body or {})))
async def main():
 db=await asyncpg.connect(DSN);other=await asyncpg.connect(DSN)
 ids=[]
 try:
  # Exclude earlier fixture work; these are synthetic rows in our owned DB.
  await db.execute("UPDATE moderation_delivery SET next_attempt_at=clock_timestamp()+interval '1 day'")
  for i in range(3):
   body={'source':'challenges','id':str(uuid4()),'case_id':str(uuid4()),'recipient':str(uuid4()),'audience':'notifier','body':{'rationale':'Synthetic safe statement','automation':'Synthetic human decision','redress':'Human review'},'contact':'synthetic-recipient@example.invalid','available_at':'2026-09-08T12:00:00Z'}
   ids.append(body['id']);assert await op(db,'accept_delivery',body)
   assert await op(db,'accept_delivery',body)
   for changed in [body|{'contact':'different@example.invalid'},body|{'available_at':'2026-09-08T12:00:01Z'}]:
    try:await op(db,'accept_delivery',changed)
    except asyncpg.RaiseError:pass
    else:raise AssertionError('conflicting exact replay accepted')
  a,b=await asyncio.gather(op(db,'claim_email'),op(other,'claim_email'))
  assert {r['id'] for r in a}.isdisjoint({r['id'] for r in b})
  rows=a+b;assert {r['id'] for r in rows}==set(ids)
  assert all(set(r)=={'source','id','generation'} for r in rows)
  first=await op(db,'admit_email',rows[0]);ack={k:first[k] for k in ['source','id','generation','attempt_id']}
  assert not await op(db,'ack_email',ack|{'generation':0,'status':'transport_accepted'})
  assert await op(db,'ack_email',ack|{'status':'uncertain'})
  assert await op(db,'ack_email',ack|{'status':'uncertain'}),'exact acknowledgement replay'
  assert not await op(db,'ack_email',ack|{'status':'transport_accepted'}),'conflicting acknowledgement replay'
  assert await db.fetchval("SELECT delivered_at IS NULL AND status='uncertain' FROM moderation_delivery WHERE source='challenges' AND id=$1",first['id'])
  await db.execute("UPDATE moderation_delivery SET lease_until=clock_timestamp()-interval '1 second',next_attempt_at=clock_timestamp()-interval '1 second' WHERE source='challenges' AND id=ANY($1::uuid[])",ids)
  reclaimed=await op(db,'claim_email');assert len(reclaimed)==3
  assert all(r['generation']==2 for r in reclaimed)
  assert not await op(db,'ack_email',ack|{'status':'transport_accepted'}),'late worker overtook new lease'
  new=await op(db,'admit_email',next(r for r in reclaimed if r['id']==first['id']));newack={k:new[k] for k in ['source','id','generation','attempt_id']}
  assert await op(db,'ack_email',newack|{'status':'transport_accepted'})
  assert await op(db,'ack_email',newack|{'status':'transport_accepted'})
  assert await db.fetchval("SELECT count(*)=1 FROM moderation_delivery_events WHERE source='challenges' AND message_id=$1 AND event='transport_accepted'",first['id'])
  assert await db.fetchval("SELECT count(*)=3 FROM moderation_delivery_events WHERE source='challenges' AND message_id=ANY($1::uuid[]) AND event='lease_expired_uncertain'",ids)
  try:await op(db,'ack_email',newack|{'status':'read_by_recipient'})
  except asyncpg.RaiseError:pass
  else:raise AssertionError('invented notification event accepted')
  print('PASS exact owner replay; concurrent disjoint claims; fenced late/duplicate/conflicting acknowledgements; expiry uncertainty and retry; immutable transport history without invented recipient information',flush=True)
 finally:
  await other.close();await db.close()
if __name__=='__main__':asyncio.run(main())
