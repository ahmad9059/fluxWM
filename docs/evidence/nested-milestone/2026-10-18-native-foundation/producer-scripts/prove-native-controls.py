import hashlib,json,subprocess
from pathlib import Path
out=Path('target/native-control-mutations');out.mkdir()
def fault(name,path,mutate,command,expected):
 folder=out/name;folder.mkdir();original=path.read_bytes();changed=mutate(original.decode()).encode();assert changed!=original
 (folder/'original.rs').write_bytes(original);(folder/'mutated.rs').write_bytes(changed)
 row=dict(command=command,original_sha256=hashlib.sha256(original).hexdigest(),mutated_sha256=hashlib.sha256(changed).hexdigest())
 try:
  path.write_bytes(changed)
  with (folder/'console.log').open('wb') as stream:result=subprocess.run(command,stdout=stream,stderr=subprocess.STDOUT,timeout=90)
  row['exit_code']=result.returncode
 finally:
  path.write_bytes(original);row['source_restored']=path.read_bytes()==original;row['restored_sha256']=hashlib.sha256(path.read_bytes()).hexdigest();(folder/'result.json').write_text(json.dumps(row,indent=2)+'\n')
 assert result.returncode==expected,row
 return row
rows=[]
rows.append(fault('lease',Path('crates/flux-native/src/system.rs'),lambda s:s.replace('let _lease = &lease;','let _ = &callback;',1),['cargo','test','-p','flux-native','--features','system','--locked','reentrant_removal_keeps_broker_and_session_until_dispatcher_fd_retires','--','--nocapture'],101))
def epoch(s):
 a=s.index('    pub fn is_ready(');b=s.index('    pub fn complete(',a)
 return s[:a]+s[a:b].replace('            && ticket.session_epoch == self.epoch\n','',1)+s[b:]
rows.append(fault('epoch',Path('crates/flux-native/src/lifecycle.rs'),epoch,['cargo','test','-p','flux-native','--features','system','--locked','pause_resume_and_changed_state_reject_old_completion','--','--nocapture'],101))
assert not Path('/dev/dri/fluxwm-audit-forbidden').exists()
# Build and trace the changed --check only. The auditor replaces the syscall
# before entry, and the deliberately nonexistent path is an extra safeguard.
p=Path('crates/flux-native/src/probe.rs');original=p.read_bytes();modified=original.decode().replace('[arg] if arg == "--check" => {','[arg] if arg == "--check" => {\n            let _ = std::fs::File::open("/dev/dri/fluxwm-audit-forbidden");',1).encode();assert original!=modified
folder=out/'probe-open';folder.mkdir();(folder/'original.rs').write_bytes(original);(folder/'mutated.rs').write_bytes(modified)
commands=[['cargo','build','-p','flux-native','--features','system','--locked'],['python3','tools/check_native_probe.py','--artifacts',str(folder/'audit')]]
row=dict(commands=commands,original_sha256=hashlib.sha256(original).hexdigest(),mutated_sha256=hashlib.sha256(modified).hexdigest())
try:
 p.write_bytes(modified)
 with (folder/'console.log').open('wb') as stream:
  build=subprocess.run(commands[0],stdout=stream,stderr=subprocess.STDOUT,timeout=90);assert build.returncode==0
  audit=subprocess.run(commands[1],stdout=stream,stderr=subprocess.STDOUT,timeout=45)
 row['build_exit_code']=build.returncode;row['audit_exit_code']=audit.returncode
finally:
 p.write_bytes(original);row['source_restored']=p.read_bytes()==original;row['restored_sha256']=hashlib.sha256(p.read_bytes()).hexdigest();(folder/'result.json').write_text(json.dumps(row,indent=2)+'\n')
assert audit.returncode==1
trace=json.loads((folder/'audit/result.json').read_text());assert trace['child_reaped'] and trace['blocked_calls']==[{'syscall':257,'reason':'native-device-open'}]
rows.append(row);(out/'summary.json').write_text(json.dumps(rows,indent=2)+'\n');print(json.dumps({'intentional_failures_verified':len(rows),'all_sources_restored':all(r['source_restored'] for r in rows)}))
