import hashlib,json,subprocess
from pathlib import Path
out=Path('target/autostart-retirement-mutation');out.mkdir()
path=Path('crates/flux-process/src/autostart.rs');original=path.read_bytes();text=original.decode()
needle='self.restart_pending\n                    || '
assert needle in text
mutated=text.replace(needle,'',1).encode()
(out/'original-autostart.rs').write_bytes(original);(out/'mutated-autostart.rs').write_bytes(mutated)
command=['cargo','test','-p','flux-process','--locked','superseded_retirement','--','--nocapture']
result={'command':command,'original_sha256':hashlib.sha256(original).hexdigest(),'mutated_sha256':hashlib.sha256(mutated).hexdigest()}
try:
    path.write_bytes(mutated)
    with (out/'console.log').open('wb') as f: p=subprocess.run(command,stdout=f,stderr=subprocess.STDOUT,timeout=45)
    result['exit_code']=p.returncode
finally:
    path.write_bytes(original)
    result['restored_sha256']=hashlib.sha256(path.read_bytes()).hexdigest()
    result['source_restored']=path.read_bytes()==original
    (out/'result.json').write_text(json.dumps(result,indent=2)+'\n')
assert p.returncode==101, result
print(json.dumps(result))
