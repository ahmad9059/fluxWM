import hashlib,json,subprocess
from pathlib import Path
out=Path('target/autostart-budget-mutation');out.mkdir()
path=Path('crates/flux-process/src/autostart.rs');original=path.read_bytes();text=original.decode();needle='slot.bytes.max(entry.bytes)';assert text.count(needle)==1
mutated=text.replace(needle,'entry.bytes',1).encode();(out/'original-autostart.rs').write_bytes(original);(out/'mutated-autostart.rs').write_bytes(mutated)
command=['cargo','test','-p','flux-process','--locked','superseded_prefix_cannot_exceed_retained_history_budget','--','--nocapture']
result={'command':command,'original_sha256':hashlib.sha256(original).hexdigest(),'mutated_sha256':hashlib.sha256(mutated).hexdigest()}
try:
 path.write_bytes(mutated)
 with (out/'console.log').open('wb') as f: p=subprocess.run(command,stdout=f,stderr=subprocess.STDOUT,timeout=45)
 result['exit_code']=p.returncode
finally:
 path.write_bytes(original);result['source_restored']=path.read_bytes()==original;result['restored_sha256']=hashlib.sha256(path.read_bytes()).hexdigest();(out/'result.json').write_text(json.dumps(result,indent=2)+'\n')
assert p.returncode==101,result
print(json.dumps(result))
