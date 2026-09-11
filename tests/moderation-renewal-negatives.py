"""Synthetic restoration does not revive absent/ended renewal authority.
Uses actual producers, renewal/rights routes and one-shot task; loopback SMTP only.
"""
import asyncio,importlib.util,json,threading,subprocess,time
from datetime import datetime,timezone
from pathlib import Path
from uuid import UUID,uuid4
import asyncpg,httpx,jwt
spec=importlib.util.spec_from_file_location('renewal',Path(__file__).with_name('moderation-renewal.py'));x=importlib.util.module_from_spec(spec);spec.loader.exec_module(x);f=x.f
class ControlledSMTP(x.AcceptedSMTP):
 reject=False
 def handle(self):
  self.wfile.write(b'220 synthetic SMTP\r\n')
  while line:=self.rfile.readline():
   verb=line.split(b' ',1)[0].strip().upper()
   if verb in [b'EHLO',b'HELO']:self.wfile.write(b'250-localhost\r\n250 8BITMIME\r\n')
   elif verb==b'DATA':
    self.wfile.write(b'354 data\r\n')
    while (line:=self.rfile.readline()) not in [b'.\r\n',b'']:pass
    self.wfile.write(b'451 synthetic rejected\r\n' if self.reject else b'250 synthetic accepted\r\n')
   elif verb==b'QUIT':self.wfile.write(b'221 bye\r\n');return
   else:self.wfile.write(b'250 OK\r\n')
async def main():
 smtp=f.SMTP(('127.0.0.1',55903),ControlledSMTP);threading.Thread(target=smtp.serve_forever,daemon=True).start();db=None
 try:
  f.start(['/nix/store/d4lznfvcd8zqxn4hc9lpw0dvfri8p4c0-valkey-9.1.1/bin/valkey-server','--port','55902','--bind','127.0.0.1','--save','','--appendonly','no'],'renewal-negative-valkey.log');await f.wait_port(55902)
  f.start([str(f.ROOT/'target/debug/academy'),'serve'],'renewal-negative-backend.log');await f.wait_port(55901)
  db=await asyncpg.connect('postgresql://l2test@127.0.0.1:55900/l2backend')
  async with httpx.AsyncClient(base_url='http://127.0.0.1:55901',timeout=20) as c:
   async def cli(*args):
    p=await asyncio.create_subprocess_exec(str(f.ROOT/'target/debug/academy'),*args,cwd=f.ROOT,env=f.ENV,stdout=asyncio.subprocess.PIPE,stderr=asyncio.subprocess.STDOUT);out=await p.communicate();assert p.returncode==0,out
   async def login(name):
    r=await c.post('/auth/sessions',json={'name_or_email':name,'password':'synthetic password'});assert r.status_code==200,r.text;return {'Authorization':'Bearer '+r.json()['access_token']}
   admin=await c.post('/auth/sessions',json={'name_or_email':'admin2','password':'secure admin2 password','mfa_code':f.totp()});assert admin.status_code==200,admin.text;staff={'Authorization':'Bearer '+admin.json()['access_token']};actor=UUID(admin.json()['user']['id'])
   internal={'Authorization':'Bearer '+jwt.encode({'aud':'shop','exp':int(time.time())+900},'synthetic-l2-shop-internal',algorithm='HS256')}
   offer=(await c.get('/shop/premium/renewal-offer')).json()
   async def setup(label,paid_seconds=3600,agreement=False):
    name='neg_'+label+'_'+uuid4().hex[:7];email=name+'@example.invalid';await cli('admin','user','create','--verified',name,email,'synthetic password')
    uid=await db.fetchval('SELECT id FROM users WHERE name=$1',name);headers=await login(name);await cli('admin','coin','add',str(uid),'--','50000')
    if paid_seconds is not None:await db.execute("INSERT INTO premium(id,user_id,since,until) VALUES($1,$2,clock_timestamp()-interval '1 month',clock_timestamp()+$3*interval '1 second')",uuid4(),uid,paid_seconds)
    consent={'plan':'MONTHLY','consent':{'request_id':str(uuid4()),'offer_id':offer['id'],'accepted':True,'withdrawal_consent':True}}
    if agreement:
     r=await c.put('/shop/premium/autopay',headers=headers,json=consent);assert r.status_code==200,r.text
    return uid,name,email,headers,consent
   async def restriction(uid):
    case=str(uuid4());await db.fetchval('SELECT backend_moderation(\'open\',$1,$2::jsonb)',actor,json.dumps({'id':case,'target_id':str(uid),'source':'own_review','private_evidence':{'facts':'Synthetic restoration boundary'}}))
    command={'case_id':case,'request_key':str(uuid4()),'expected_revision':0,'outcome':'restrict','rationale':'Synthetic restriction','ground':'Synthetic security ground','rule_version':'Synthetic only','automation':'Synthetic human','scope':'Allgemeiner Kontozugang auf Bootstrap Academy; Rechtezugang bleibt erhalten','redress':'At least six calendar months human review','misconduct_facts':'Synthetic established misconduct','proportionality':'Synthetic assessment','hearing':'Synthetic hearing'}
    r=await c.post('/auth/moderation/admin/decide',headers=staff,json=command);assert r.status_code==200,r.text;return command
   async def restore(command):
    r=await c.post('/auth/moderation/admin/decide',headers=staff,json=command|{'request_key':str(uuid4()),'expected_revision':1,'outcome':'restore'});assert r.status_code==200,r.text
   async def snapshot(uid):return await db.fetchval("SELECT jsonb_build_object('coins',(SELECT to_jsonb(c) FROM coins c WHERE user_id=$1),'periods',(SELECT jsonb_agg(to_jsonb(p) ORDER BY id) FROM premium p WHERE user_id=$1),'subscriptions',(SELECT jsonb_agg(to_jsonb(s)) FROM premium_subscriptions s WHERE user_id=$1),'agreements',(SELECT jsonb_agg(to_jsonb(a) ORDER BY id) FROM premium_renewal_agreements a WHERE user_id=$1),'cancellations',(SELECT jsonb_agg(to_jsonb(c) ORDER BY agreement_id) FROM premium_renewal_cancellations c JOIN premium_renewal_agreements a ON a.id=c.agreement_id WHERE a.user_id=$1),'ledger',(SELECT jsonb_agg(to_jsonb(t) ORDER BY id) FROM transactions t WHERE user_id=$1))::text",uid)
   async def no_renewal(uid,before):
    r=await c.get('/shop/_internal/premium/'+str(uid),headers=internal);assert r.status_code==200 and r.json() is False,r.text
    await cli('task','refresh-premium');assert await snapshot(uid)==before
   for label,plan in [('off',None),('legacy_monthly','monthly'),('legacy_yearly','yearly'),('no_paid',None)]:
    uid,name,email,headers,consent=await setup(label,None if label=='no_paid' else -1)
    if plan:await db.execute('INSERT INTO premium_subscriptions(user_id,plan,agreement_id) VALUES($1,$2::premium_plan,NULL)',uid,plan)
    if label=='no_paid':
     r=await c.put('/shop/premium/autopay',headers=headers,json=consent);assert r.status_code==412,r.text
    before=await snapshot(uid);command=await restriction(uid);await restore(command);await no_renewal(uid,before)
   print('PASS actual restore/internal/task matrix: OFF, missing paid membership and legacy monthly/yearly without agreement remain uncharged and unextended; no-paid consent cannot create renewal',flush=True)
   uid,name,email,headers,consent=await setup('cancelled',agreement=True)
   r=await c.put('/shop/premium/autopay',headers=headers,json={'plan':None});assert r.status_code==200,r.text
   await db.execute("UPDATE premium SET until=clock_timestamp()-interval '1 second' WHERE user_id=$1",uid)
   before=await snapshot(uid);command=await restriction(uid);await restore(command)
   r=await c.put('/shop/premium/autopay',headers=await login(name),json=consent);assert r.status_code==200,r.text
   await no_renewal(uid,before)
   print('PASS actual OFF of confirmed agreement survives restriction/restoration and exact original consent replay; no new subscription, debit or period',flush=True)
   uid,name,email,headers,consent=await setup('funded_later',agreement=True)
   await db.execute("UPDATE premium SET until=clock_timestamp()-interval '1 second' WHERE user_id=$1",uid);await db.execute('UPDATE coins SET coins=0 WHERE user_id=$1',uid)
   r=await c.get('/shop/_internal/premium/'+str(uid),headers=internal);assert r.status_code==200 and r.json() is False,r.text
   assert await db.fetchval('SELECT EXISTS(SELECT 1 FROM premium_renewal_cancellations WHERE agreement_id=$1)',UUID(consent['consent']['request_id']))
   await cli('admin','coin','add',str(uid),'--','50000');before=await snapshot(uid);command=await restriction(uid);await restore(command);await no_renewal(uid,before)
   print('PASS actual insufficient-funds cancellation remains ended after later funding and restriction/restoration',flush=True)
   ControlledSMTP.reject=True
   uid,name,email,headers,consent=await setup('late_confirmation',paid_seconds=5,agreement=True);aid=UUID(consent['consent']['request_id'])
   original=await db.fetchval('SELECT to_jsonb(a)::text FROM premium_renewal_agreements a WHERE id=$1',aid)
   assert await db.fetchval('SELECT sent_at IS NULL FROM premium_renewal_delivery WHERE agreement_id=$1',aid)
   deadline=await db.fetchval('SELECT confirmation_deadline FROM premium_renewal_agreements WHERE id=$1',aid);command=await restriction(uid)
   await asyncio.sleep(max(0,(deadline-datetime.now(timezone.utc)).total_seconds())+.1)
   # Read during restriction reconciles the actual missed original deadline; it
   # cannot be extended by restoration or a later transport success.
   r=await c.get('/shop/_internal/premium/'+str(uid),headers=internal);assert r.status_code==200 and r.json() is False
   before=await snapshot(uid);ControlledSMTP.reject=False;await restore(command);await no_renewal(uid,before)
   assert await db.fetchval('SELECT to_jsonb(a)::text FROM premium_renewal_agreements a WHERE id=$1',aid)==original
   print('PASS original confirmation deadline passes during restriction: late transport recovery/restoration cannot revive or rewrite agreement, paid deadline, confirmation body or PDF originals',flush=True)
   uid,name,email,headers,consent=await setup('statutory',paid_seconds=5,agreement=True);aid=UUID(consent['consent']['request_id']);command=await restriction(uid)
   body={'name':'Synthetic owner declaration','email':email,'contract':'PREMIUM','cancellation_type':'ORDINARY','renewal_agreement_id':str(aid),'details':'Synthetic statutory cancellation during restriction','request_key':{'id':str(uuid4()),'secret':str(uuid4())}}
   receipt=await c.post('/contracts/cancellations',json=body);assert receipt.status_code==200,receipt.text;original_receipt=receipt.json()['declaration']
   action={'identity_verified':True,'action':'SCHEDULE_PREMIUM_CANCELLATION','verified_user_id':str(uid),'renewal_agreement_id':str(aid),'note':'Synthetic owner and exact original agreement independently verified'}
   processed=await c.patch('/contracts/declarations/'+body['request_key']['id'],headers=staff,json=action);assert processed.status_code==409,processed.text
   # The direct paid-row fixture intentionally has no historical COMMIT witness.
   # Exact-agreement cessation still applies; its original temporal uncertainty
   # must remain pending rather than being converted into a human finding.
   uncertainty=await db.fetchval('SELECT processing_note FROM contract_declarations WHERE id=$1',UUID(body['request_key']['id']));assert uncertainty
   assert await db.fetchval('SELECT completed_at IS NOT NULL FROM contract_cancellation_schedule WHERE declaration_id=$1',UUID(body['request_key']['id']))
   until=await db.fetchval('SELECT max(until) FROM premium WHERE user_id=$1',uid);await asyncio.sleep(max(0,(until-datetime.now(timezone.utc)).total_seconds())+.1)
   await restore(command);r=await c.get('/shop/_internal/premium/'+str(uid),headers=internal);assert r.status_code==200 and r.json() is False,r.text
   before=await snapshot(uid);await no_renewal(uid,before)
   retained=await c.post('/contracts/receipts',json=body['request_key']);assert retained.status_code==200 and retained.json()['declaration']==original_receipt,retained.text
   assert await db.fetchval('SELECT received_at FROM contract_declarations WHERE id=$1',UUID(body['request_key']['id']))==datetime.fromisoformat(original_receipt['received_at'].replace('Z','+00:00'))
   assert await db.fetchval('SELECT processing_note FROM contract_declarations WHERE id=$1',UUID(body['request_key']['id']))==uncertainty
   print('PASS actual public T12 exact-agreement cancellation during restriction prevents renewal after restoration; original declaration/time and unresolved fixture period-provenance review remain unchanged, with no invented human resolution',flush=True)
 finally:
  if db:await db.close()
  for p in reversed(f.CHILDREN):
   if p.poll() is None:p.terminate()
   try:p.wait(timeout=15)
   except subprocess.TimeoutExpired:p.kill();p.wait()
  smtp.shutdown();smtp.server_close()
if __name__=='__main__':asyncio.run(main())
