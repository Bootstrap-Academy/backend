"""Actual restricted routes and PostgreSQL concurrency on the owned L2 fixture.
External services are explicitly synthetic loopback export boundaries; no mail is delivered.
"""
import asyncio, hashlib, importlib.util, json, threading, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from uuid import UUID, uuid4
import asyncpg, httpx
spec=importlib.util.spec_from_file_location('fixture',Path(__file__).with_name('moderation-http.py'));f=importlib.util.module_from_spec(spec);spec.loader.exec_module(f)
class ExportBoundary(BaseHTTPRequestHandler):
 fail_moderation=False
 def log_message(self,*args):pass
 def do_GET(self):
  if self.path.endswith('/export'):time.sleep(.2);status,value=200,{'synthetic_export':True}
  elif self.path.endswith('/inbox'):status,value=(503,{'detail':'synthetic moderation-only outage'}) if self.fail_moderation else (200,[])
  else:status,value=200,[]
  self.send_response(status);self.send_header('Content-Type','application/json');self.end_headers();self.wfile.write(json.dumps(value).encode())
 def do_POST(self):
  self.rfile.read(int(self.headers.get('Content-Length',0)));self.send_response(200);self.send_header('Content-Type','application/json');self.end_headers();self.wfile.write(b'[]')
async def main():
 servers=[];db=other=None
 try:
  for port in [55904,55905,55906]:
   server=ThreadingHTTPServer(('127.0.0.1',port),ExportBoundary);server.daemon_threads=True;threading.Thread(target=server.serve_forever,daemon=True).start();servers.append(server)
  smtp=f.SMTP(('127.0.0.1',55903),f.SMTPHandler);threading.Thread(target=smtp.serve_forever,daemon=True).start();servers.append(smtp)
  f.start(['/nix/store/d4lznfvcd8zqxn4hc9lpw0dvfri8p4c0-valkey-9.1.1/bin/valkey-server','--port','55902','--bind','127.0.0.1','--save','','--appendonly','no'],'retained-valkey.log');await f.wait_port(55902)
  f.start([str(f.ROOT/'target/debug/academy'),'serve'],'retained-backend.log');await f.wait_port(55901)
  db=await asyncpg.connect('postgresql://l2test@127.0.0.1:55900/l2backend');other=await asyncpg.connect('postgresql://l2test@127.0.0.1:55900/l2backend')
  async def op(conn,name,body,actor=None):return json.loads(await conn.fetchval('SELECT backend_moderation($1,$2,$3::jsonb)::text',name,actor,json.dumps(body)))
  async with httpx.AsyncClient(base_url='http://127.0.0.1:55901',timeout=12) as client:
   proof=await client.post('/auth/moderation/access/password',json={'name_or_email':'foo','password':'foo password'});assert proof.status_code==200,proof.text
   rights={'x-moderation-capability':proof.json()['capability']}
   exports=await asyncio.wait_for(asyncio.gather(*(client.get('/auth/moderation/export',headers=rights) for _ in range(4))),10)
   assert all(r.status_code==200 and r.json()['complete'] for r in exports),[(r.status_code,r.text[:100]) for r in exports]
   ExportBoundary.fail_moderation=True
   partial=await client.get('/auth/moderation/export',headers=rights);assert partial.status_code==200,partial.text
   assert all(v is not None for v in partial.json()['services'].values()) and not partial.json()['complete']
   assert not partial.json()['moderation']['challenges_available']
   ExportBoundary.fail_moderation=False
   print('PASS actual 4 concurrent retained exports with backend pool max_connections=1; complete=false when all service exports succeed but later moderation fails',flush=True)
   # A valid principal, then a genuine local inbox query cancellation. This
   # produces an infrastructure failure through the actual route, not a mock401.
   lock=db.transaction();await lock.start();await db.execute('LOCK moderation_messages IN ACCESS EXCLUSIVE MODE')
   pending=asyncio.create_task(client.get('/auth/moderation/inbox',headers=rights))
   for _ in range(100):
    pid=await other.fetchval("SELECT pid FROM pg_stat_activity WHERE datname='l2backend' AND wait_event_type='Lock' AND query LIKE 'SELECT backend_moderation%' LIMIT 1")
    if pid:break
    await asyncio.sleep(.02)
   assert pid,'actual inbox never reached its blocked SQL read'
   await other.execute('SELECT pg_cancel_backend($1)',pid);failed=await pending;await lock.rollback()
   assert failed.status_code==503,failed.text
   assert (await client.get('/auth/moderation/inbox',headers=rights)).status_code==200
   print('PASS actual local inbox infrastructure failure503 and same valid capability succeeds after recovery',flush=True)
   person=uuid4();case=uuid4();messages=[]
   for _ in range(2):
    message={'source':'challenges','id':str(uuid4()),'case_id':str(case),'recipient':str(person),'audience':'notifier','body':{'text':'Synthetic notifier outcome','automation':'Human synthetic assessment','redress':'Six months human review'},'contact':'old@example.invalid','available_at':'2026-09-08T00:00:00Z'}
    messages.append(message);await op(db,'accept_delivery',message)
   async def recover(contact):
    r=await client.post('/auth/moderation/access/recovery',json={'source':'challenges','case_id':str(case),'contact':contact});assert r.status_code==200,r.text
   await recover('old@example.invalid')
   oldcap=await db.fetchval("SELECT body->>'recovery_link' FROM moderation_delivery WHERE case_id=$1 AND audience='recovery' ORDER BY accepted_at DESC LIMIT 1",case);assert oldcap
   oldheaders={'x-moderation-capability':oldcap.split('capability=')[1]}
   assert (await client.get('/auth/moderation/inbox',headers=oldheaders)).status_code==200
   # Hold the real correction transaction open, then submit recovery concurrently.
   tx=db.transaction();await tx.start()
   await op(db,'delivery_contact',{'source':'challenges','id':messages[1]['id'],'contact':'new@example.invalid','verification_evidence':'Synthetic verified contact correction'},uuid4())
   attempt=asyncio.create_task(recover('old@example.invalid'));await asyncio.sleep(.2);assert not attempt.done(),'recovery did not use the correction authority fence'
   await tx.commit();await attempt
   assert (await client.get('/auth/moderation/inbox',headers=oldheaders)).status_code==401,'previously issued case proof survived correction'
   before=await db.fetchval('SELECT count(*) FROM moderation_capabilities WHERE case_id=$1',case)
   await recover('old@example.invalid');assert await db.fetchval('SELECT count(*) FROM moderation_capabilities WHERE case_id=$1',case)==before
   await recover('new@example.invalid');assert await db.fetchval('SELECT count(*) FROM moderation_capabilities WHERE case_id=$1',case)==before+1
   link=await db.fetchval("SELECT body->>'recovery_link' FROM moderation_delivery WHERE case_id=$1 AND audience='recovery' ORDER BY accepted_at DESC LIMIT 1",case);case_headers={'x-moderation-capability':link.split('capability=')[1]}
   inbox=await client.get('/auth/moderation/inbox',headers=case_headers);assert inbox.status_code==200 and inbox.json()['scope']=='case' and inbox.json()['recipient_id']==str(person)
   for method,path in [('GET','/export'),('POST','/finance-access'),('DELETE','/account'),('DELETE','/events/'+str(uuid4())),('GET','/purchases/'+str(uuid4())+'/documents/terms')]:
    response=await client.request(method,'/auth/moderation'+path,headers=case_headers);assert response.status_code==403,(path,response.status_code,response.text)
   assert (await client.post('/auth/moderation/complaints/backend',headers=case_headers,json={'id':str(uuid4()),'decision_id':str(uuid4()),'text':'Wrong case'})).status_code==403
   assert await db.fetchval("SELECT count(*)=2 FROM moderation_delivery WHERE case_id=$1 AND audience='notifier' AND contact='old@example.invalid'",case),'original transport address rewritten'
   assert not await db.fetchval('SELECT EXISTS(SELECT 1 FROM users WHERE id=$1)',person),'deleted/non-account recipient accidentally registered'
   print('PASS multi-message corrected contact retires historical recovery authority and existing case capabilities; new address recovers deleted/non-account notifier; stronger account/finance/purchase/event rights and unrelated case denied',flush=True)
   # Early contact minimization is independently serialized and fences both
   # exact old replay and a previously unadopted message from the same case.
   minimum={'source':'challenges','id':str(uuid4()),'case_id':str(case),'field':'notifier_contact'}
   tx=db.transaction();await tx.start();await op(db,'accept_minimization',minimum)
   recovering=asyncio.create_task(recover('new@example.invalid'));replaying=asyncio.create_task(op(other,'accept_delivery',messages[0]));await asyncio.sleep(.2);assert not recovering.done() and not replaying.done()
   await tx.commit();await asyncio.gather(recovering,replaying)
   assert (await client.get('/auth/moderation/inbox',headers=case_headers)).status_code==401
   delayed=messages[0]|{'id':str(uuid4())};await op(db,'accept_delivery',delayed);messages.append(delayed)
   assert await op(db,'accept_minimization',minimum)
   assert not await db.fetchval("SELECT EXISTS(SELECT 1 FROM moderation_delivery WHERE case_id=$1 AND (contact IS NOT NULL OR owner_contact IS NOT NULL OR body ? 'recovery_link'))",case)
   assert not await db.fetchval('SELECT EXISTS(SELECT 1 FROM moderation_capabilities WHERE case_id=$1 AND revoked_at IS NULL)',case)
   # A command that arrives before any owner message must fence first adoption.
   never=uuid4();await op(db,'accept_minimization',minimum|{'id':str(uuid4()),'case_id':str(never)})
   await op(db,'accept_delivery',delayed|{'id':str(uuid4()),'case_id':str(never)})
   assert not await db.fetchval('SELECT EXISTS(SELECT 1 FROM moderation_recipient_contacts WHERE case_id=$1 AND contact IS NOT NULL)',never)
   print('PASS early contact minimization fences actual concurrent recovery, exact stale replay and unseen/first owner adoption; plaintext contacts/links and old case authority cannot resurrect',flush=True)
   tx=db.transaction();await tx.start();await op(db,'accept_disposal',{'source':'challenges','case_id':str(case)})
   recovering=asyncio.create_task(recover('new@example.invalid'));adopting=asyncio.create_task(op(other,'accept_delivery',messages[0]));await asyncio.sleep(.2);assert not recovering.done() and not adopting.done()
   await tx.commit();await asyncio.gather(recovering,adopting)
   for table in ['moderation_delivery','moderation_capabilities','moderation_recipient_contacts']:
    assert await db.fetchval(f'SELECT count(*) FROM {table} WHERE case_id=$1',case)==0,table
   assert not await db.fetchval('SELECT EXISTS(SELECT 1 FROM moderation_delivery_events WHERE message_id=ANY($1::uuid[]))',[UUID(m['id']) for m in messages])
   print('PASS committed disposal fences concurrent stale relay and recovery; no body/contact/capability/history resurrection',flush=True)
   uid=await db.fetchval("SELECT id FROM users WHERE name='foo'");local_case=uuid4();actor=uuid4()
   await op(db,'open',{'id':str(local_case),'target_id':str(uid),'source':'own_review','private_evidence':{'facts':'Synthetic local disposal test'}},actor)
   command={'request_key':str(uuid4()),'case_id':str(local_case),'expected_revision':0,'outcome':'restrict','misconduct_facts':'Synthetic independently verified account security finding','proportionality':'Synthetic necessary limited restriction','hearing':'Synthetic concrete immediate urgency assessed','rationale':'Synthetic local restriction','ground':'Synthetic ground','rule_version':'Synthetic only','automation':'Synthetic human decision','scope':'Allgemeiner Kontozugang auf Bootstrap Academy; Rechtezugang bleibt erhalten','redress':'Six months human review'}
   await op(db,'decide',command,actor)
   await op(db,'decide',command|{'request_key':str(uuid4()),'expected_revision':1,'outcome':'restore'},actor)
   await db.execute("UPDATE moderation_messages SET informed_at=clock_timestamp()-interval '2 years',complaint_until=clock_timestamp()-interval '1 year' WHERE case_id=$1",local_case)
   await op(db,'maintenance',{})
   assert await db.fetchval('SELECT closed_at IS NULL FROM moderation_cases WHERE id=$1',local_case),'pending commercial work incorrectly closed case'
   await op(db,'handling',{'id':str(uuid4()),'case_id':str(local_case),'kind':'commercial_rights_and_contract_review','status':'not_applicable','facts':'Synthetic account has no affected paid obligations'},actor)
   await op(db,'maintenance',{});assert await db.fetchval('SELECT closed_at IS NOT NULL FROM moderation_cases WHERE id=$1',local_case)
   claims=await op(db,'claim',{});claimed=next(m for m in claims if m['case_id']==str(local_case));claimed['source']='backend'
   await op(db,'accept_delivery',claimed)
   await db.execute("UPDATE moderation_cases SET closed_at=clock_timestamp()-interval '13 months' WHERE id=$1",local_case)
   disposal={'id':str(uuid4()),'case_id':str(local_case),'action':'dispose','reason':'Synthetic actual closed case review','claims_and_retention_checked':True}
   tx=db.transaction();await tx.start();await op(db,'retention',disposal,actor)
   importing=asyncio.create_task(op(other,'accept_delivery',claimed))
   recovering=asyncio.create_task(client.post('/auth/moderation/access/recovery',json={'source':'backend','case_id':str(local_case),'contact':claimed['contact']}))
   await asyncio.sleep(.2);assert not importing.done() and not recovering.done()
   await tx.commit();await importing;assert (await recovering).status_code==200
   assert await op(db,'retention',disposal,actor)
   for table in ['moderation_delivery','moderation_capabilities','moderation_recipient_contacts','moderation_handling_events']:
    assert await db.fetchval(f'SELECT count(*) FROM {table} WHERE case_id=$1',local_case)==0,table
   print('PASS backend-local closure waits for commercial work; committed local disposal fences stale owner adoption and concurrent recovery, with durable exact replay receipt',flush=True)

 finally:
  for connection in [db,other]:
   if connection:await connection.close()
  for child in reversed(f.CHILDREN):
   child.terminate()
   try:child.wait(timeout=15)
   except:child.kill();child.wait()
  for server in servers:server.shutdown();server.server_close()
if __name__=='__main__':asyncio.run(main())
