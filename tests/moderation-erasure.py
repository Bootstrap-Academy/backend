"""Opt-in actual T10 Challenges erasure admission under the new moderation guard.
Only the owned L2 PostgreSQL/Valkey and loopback process fixture are used.
"""
import asyncio, importlib.util, json, os, subprocess, threading, time
from pathlib import Path
from uuid import uuid4
import asyncpg, httpx, jwt
spec=importlib.util.spec_from_file_location('consumers',Path(__file__).with_name('moderation-consumers.py'));x=importlib.util.module_from_spec(spec);spec.loader.exec_module(x);f=x.f
async def main():
 server=x.ThreadingHTTPServer(('127.0.0.1',55907),x.Sandbox);threading.Thread(target=server.serve_forever,daemon=True).start()
 try:
  f.start(['/nix/store/d4lznfvcd8zqxn4hc9lpw0dvfri8p4c0-valkey-9.1.1/bin/valkey-server','--port','55902','--bind','127.0.0.1','--save','','--appendonly','no'],'erasure-valkey.log');await f.wait_port(55902)
  proc=subprocess.Popen([str(x.WORK/'challenges-ms/target/debug/challenges')],cwd=x.WORK/'challenges-ms',env=os.environ|{'CONFIG_PATH':str(f.BASE/'challenges.toml'),'RUST_LOG':'warn'},stdout=(f.BASE/'erasure-challenges.log').open('w'),stderr=subprocess.STDOUT);f.CHILDREN.append(proc);await f.wait_port(55904)
  db=await asyncpg.connect('postgresql://l2test@127.0.0.1:55900/l2challenges')
  author,other,parent,target,foreign,case,moderator=[uuid4() for _ in range(7)]
  async with db.transaction():
   await db.execute('INSERT INTO challenges_tasks VALUES($1,$2,now())',parent,author)
   for subtask,owner in [(target,author),(foreign,other)]:await db.execute("INSERT INTO challenges_subtasks(id,task_id,creator,creation_timestamp,xp,coins,ty) VALUES($1,$2,$3,now(),0,0,'question')",subtask,parent,owner)
   await db.execute("SELECT moderation_open($1,$2,'subtask',$3,$4,'own_review',NULL,'{}')",case,moderator,target,author)
   command={'request_key':str(uuid4()),'case_id':str(case),'expected_revision':0,'reviewed_content_revision':0,'outcome':'provisional','rationale':'Synthetic reason for retained deletion history','ground':'Synthetic independently assessed ground','rule_version':'Synthetic fixture only','automation':'Human synthetic command','scope':'Diese Teilaufgabe auf Bootstrap Academy','redress':'Six months human review'}
   await db.execute('SELECT moderation_decide($1,$2::jsonb)',moderator,json.dumps(command))
  original=await db.fetchval('SELECT public_statement::text FROM moderation_decisions WHERE case_id=$1',case)
  token=jwt.encode({'aud':'challenges','exp':int(time.time())+300},'synthetic-l2-challenges-internal',algorithm='HS256')
  async with httpx.AsyncClient(timeout=15) as c:
   for _ in range(2):
    r=await c.delete('http://127.0.0.1:55904/_internal/users/'+str(author),headers={'Authorization':'Bearer '+token});assert r.status_code==204,(r.status_code,r.text)
  assert not await db.fetchval('SELECT EXISTS(SELECT 1 FROM challenges_subtasks WHERE id=$1)',target)
  assert await db.fetchval('SELECT EXISTS(SELECT 1 FROM challenges_subtasks WHERE id=$1)',foreign)
  assert await db.fetchval('SELECT EXISTS(SELECT 1 FROM challenges_tasks WHERE id=$1)',parent)
  assert await db.fetchval("SELECT withdrawn FROM moderation_targets WHERE kind='subtask' AND id=$1",target)
  assert await db.fetchval('SELECT public_statement::text FROM moderation_decisions WHERE case_id=$1',case)==original
  assert not await db.fetchval('SELECT active FROM moderation_holds WHERE case_id=$1',case)
  print('PASS actual internal Challenges T10 deletion and replay204 cross the authenticated erasure guard; own target removed, foreign sibling/shared parent preserved, immutable statement survives and ordinary hold withdrawn',flush=True)
  await db.close()
 finally:
  for p in reversed(f.CHILDREN):
   if p.poll() is None:p.terminate()
   try:p.wait(timeout=15)
   except subprocess.TimeoutExpired:p.kill();p.wait()
  server.shutdown();server.server_close()
if __name__=='__main__':asyncio.run(main())
