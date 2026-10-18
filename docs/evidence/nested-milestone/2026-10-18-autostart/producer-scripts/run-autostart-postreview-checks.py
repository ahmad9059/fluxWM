import hashlib,json,subprocess,time
from pathlib import Path
out=Path('target/autostart-postreview-checks');out.mkdir()
commands=[('tests',['cargo','test','--workspace','--locked']),('test-list',['cargo','test','--workspace','--locked','--','--list']),('clippy',['cargo','clippy','--workspace','--all-targets','--locked','--','-D','warnings']),('rustdoc',['cargo','doc','--workspace','--no-deps','--locked']),('fmt',['cargo','fmt','--all','--','--check']),('docs',['python3','tools/check_docs.py']),('diff',['git','diff','--check']),('build',['cargo','build','--workspace','--locked'])]
rows=[]
for name,command in commands:
 import os
 env=dict(os.environ)
 if name=='rustdoc':env['RUSTDOCFLAGS']='-D warnings'
 started=time.monotonic()
 with (out/(name+'.log')).open('wb') as stream:p=subprocess.run(command,stdout=stream,stderr=subprocess.STDOUT,env=env,timeout=180)
 rows.append(dict(name=name,command=command,exit_code=p.returncode,elapsed_seconds=round(time.monotonic()-started,3)))
 (out/'checks.json').write_text(json.dumps(rows,indent=2)+'\n')
 if p.returncode:raise SystemExit(p.returncode)
paths=[Path('target/debug')/n for n in ['fluxwm','fluxwm-config','fluxwm-fixture']]
(out/'executables.json').write_text(json.dumps({str(p.resolve()):hashlib.sha256(p.read_bytes()).hexdigest() for p in paths},indent=2)+'\n')
print(json.dumps({'ok':True,'checks':len(rows)}))
