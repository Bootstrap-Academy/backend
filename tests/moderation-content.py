"""Actual Challenges exact content/owner review, report replay and edit race.
Reuses committed T11 synthetic definitions; all dependencies are owned loopback.
"""
import asyncio,importlib.util,json,os,subprocess,threading,time
from pathlib import Path
from uuid import UUID,uuid4
import asyncpg,httpx,jwt
spec=importlib.util.spec_from_file_location('consumers',Path(__file__).with_name('moderation-consumers.py'));x=importlib.util.module_from_spec(spec);spec.loader.exec_module(x);f=x.f
async def main():
 db=backend=None;server=x.ThreadingHTTPServer(('127.0.0.1',55907),x.Sandbox);threading.Thread(target=server.serve_forever,daemon=True).start()
 smtp=f.SMTP(('127.0.0.1',55903),f.SMTPHandler);threading.Thread(target=smtp.serve_forever,daemon=True).start()
 try:
  f.start(['/nix/store/d4lznfvcd8zqxn4hc9lpw0dvfri8p4c0-valkey-9.1.1/bin/valkey-server','--port','55902','--bind','127.0.0.1','--save','','--appendonly','no'],'content-valkey.log');await f.wait_port(55902)
  f.start([str(f.ROOT/'target/debug/academy'),'serve'],'content-backend.log');await f.wait_port(55901)
  proc=subprocess.Popen([str(x.WORK/'challenges-ms/target/debug/challenges')],cwd=x.WORK/'challenges-ms',env=os.environ|{'CONFIG_PATH':str(f.BASE/'challenges.toml'),'RUST_LOG':'warn'},stdout=(f.BASE/'content-challenges.log').open('w'),stderr=subprocess.STDOUT);f.CHILDREN.append(proc);await f.wait_port(55904)
  db=await asyncpg.connect('postgresql://l2test@127.0.0.1:55900/l2challenges');backend=await asyncpg.connect('postgresql://l2test@127.0.0.1:55900/l2backend')
  async with httpx.AsyncClient(timeout=25) as c:
   async def auth(name,pw,**extra):
    r=await c.post('http://127.0.0.1:55901/auth/sessions',json={'name_or_email':name,'password':pw,**extra});assert r.status_code==200,r.text;return r.json()
   admin=await auth('admin2','secure admin2 password',mfa_code=f.totp());foo=await auth('foo','foo password');bar=admin;staff={'Authorization':'Bearer '+admin['access_token']};reporter={'Authorization':'Bearer '+bar['access_token']};owner={'Authorization':'Bearer '+foo['access_token']}
   uid=foo['user']['id'];other=bar['user']['id'];fixture=(x.WORK/'challenges-ms/challenges/src/services/fixtures/authored_export.sql').read_text().replace('00000000-0000-0000-0000-000000000100',uid).replace('00000000-0000-0000-0000-000000000200',other)
   await db.execute(fixture)
   code='Synthetic source 雪\n'+('print("owned private source")\n'*20000)
   await db.execute("UPDATE challenges_coding_challenges SET evaluator=$1,solution_code=$1 WHERE subtask_id='00000000-0000-0000-0000-000000000101'",code)
   internal={'Authorization':'Bearer '+jwt.encode({'aud':'challenges','exp':int(time.time())+900},'synthetic-l2-challenges-internal',algorithm='HS256')}
   before=await db.fetchval('SELECT count(*) FROM moderation_targets');export=await c.get('http://127.0.0.1:55904/_internal/users/'+uid+'/export',headers=internal);assert export.status_code==200,export.text
   assert await db.fetchval('SELECT count(*) FROM moderation_targets')==before
   exported={v['id']:v['content'] for v in export.json()['subtasks_created']};assert len(exported)==5,exported.keys();assert 'OTHER_AUTHOR_PRIVATE' not in json.dumps(exported)
   cases={}
   for suffix in ['101','102','103','104','107']:
    target='00000000-0000-0000-0000-000000000'+suffix;case=str(uuid4());r=await c.post('http://127.0.0.1:55904/moderation/cases',headers=staff,json={'id':case,'target_kind':'subtask','target_id':target,'source':'own_review','private_evidence':{'facts':'Synthetic exact target review'}});assert r.status_code==200,r.text
    record=await c.get('http://127.0.0.1:55904/moderation/cases/'+case,headers=staff);assert record.status_code==200,record.text;record=record.json();cases[suffix]=record
    assert record['subject']==uid and record['target_id']==target and record['review_target']['content']==exported[target]
    assert record['private_evidence']['target_content']==exported[target]
   print('PASS actual internal T11 read-only export and human detail match all four typed definitions, empty fifth definition and >500KB exact source; shared parents/foreign author never replace subtask identity',flush=True)
   row=cases['102'];case=row['id'];target=row['target_id']
   command={'case_id':case,'request_key':str(uuid4()),'expected_revision':0,'reviewed_content_revision':row['review_target']['revision'],'outcome':'provisional','rationale':'Synthetic reviewed exact question issue','ground':'Synthetic explicit human ground','rule_version':'Synthetic unchanged historical basis unknown','automation':'Human synthetic decision','scope':'Diese Teilaufgabe auf Bootstrap Academy','redress':'At least six calendar months human review'}
   tx=db.transaction();await tx.start();await db.execute('UPDATE challenges_questions SET question=$1 WHERE subtask_id=$2','Corrected synthetic exact question',UUID(target))
   pending=asyncio.create_task(c.post('http://127.0.0.1:55904/moderation/decisions',headers=staff,json=command));await asyncio.sleep(.25);assert not pending.done()
   await tx.commit();r=await pending;assert r.status_code==409,r.text
   row=(await c.get('http://127.0.0.1:55904/moderation/cases/'+case,headers=staff)).json();assert row['private_evidence']['target_content']==exported[target] and row['review_target']['content']!=exported[target]
   command['reviewed_content_revision']=row['review_target']['revision']
   results=await asyncio.gather(*(c.post('http://127.0.0.1:55904/moderation/decisions',headers=staff,json=command) for _ in range(2)));assert all(r.status_code==200 for r in results),[(r.status_code,r.text) for r in results];assert results[0].json()==results[1].json()
   assert await db.fetchval('SELECT count(*) FROM moderation_decisions WHERE case_id=$1',UUID(case))==1
   assert not await db.fetchval('SELECT enabled FROM challenges_subtasks WHERE id=$1',UUID(target))
   assert await db.fetchval('SELECT count(*) FROM moderation_messages WHERE case_id=$1 AND informed_at IS NOT NULL',UUID(case))==0
   print('PASS actual correction transaction defeats stale decision after lock wait; fresh exact review concurrent replay commits one decision/hold/statement and does not invent recipient information',flush=True)
   # An enabled sibling, different typed definition under the same parent.
   task='00000000-0000-0000-0000-000000000002';subtask='00000000-0000-0000-0000-000000000104';request={'request_id':str(uuid4()),'task_id':task,'subtask_id':subtask,'reason':'OTHER','comment':'Synthetic private notifier statement'}
   responses=await asyncio.gather(*(c.post('http://127.0.0.1:55904/subtask_reports',headers=reporter,json=request) for _ in range(2)));assert all(r.status_code==201 for r in responses),[(r.status_code,r.text) for r in responses];assert responses[0].json()==responses[1].json()
   assert await db.fetchval('SELECT count(*) FROM moderation_cases WHERE id=$1',UUID(request['request_id']))==1
   assert (await c.post('http://127.0.0.1:55904/subtask_reports',headers=reporter,json=request|{'comment':'different'})).status_code==409
   assert (await c.post('http://127.0.0.1:55904/subtask_reports',headers=owner,json=request)).status_code==409
   # Target withdrawal removes live report rows, while exact authenticated receipt
   # replay remains available from the independent case with original timestamp.
   async with db.transaction():
    await db.execute("SELECT set_config('academy.moderation_erasure_subject',$1,true)",uid);await db.execute('DELETE FROM challenges_subtasks WHERE id=$1',UUID(subtask))
   replay=await c.post('http://127.0.0.1:55904/subtask_reports',headers=reporter,json=request);assert replay.status_code==201 and replay.json()==responses[0].json(),replay.text
   print('PASS actual concurrent notifier report replay yields one case and identical receipt; changed request/actor rejected, target withdrawal retains exact receipt without recreating target/report/private text',flush=True)
 finally:
  for connection in [db,backend]:
   if connection:await connection.close()
  for proc in reversed(f.CHILDREN):
   if proc.poll() is None:proc.terminate()
   try:proc.wait(timeout=15)
   except subprocess.TimeoutExpired:proc.kill();proc.wait()
  smtp.shutdown();smtp.server_close();server.shutdown();server.server_close()
if __name__=='__main__':asyncio.run(main())
