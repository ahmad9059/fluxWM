import hashlib,json,os,subprocess,time
from pathlib import Path
out=Path('target/native-output-release-checks');out.mkdir()
commands=[('default-tests',['cargo','test','--workspace','--locked']),('native-tests',['cargo','test','--workspace','--all-features','--locked']),('native-test-list',['cargo','test','--workspace','--all-features','--locked','--','--list']),('check',['cargo','check','--workspace','--all-features','--all-targets','--locked']),('clippy',['cargo','clippy','--workspace','--all-features','--all-targets','--locked','--','-D','warnings']),('rustdoc',['cargo','doc','--workspace','--all-features','--no-deps','--locked']),('fmt',['cargo','fmt','--all','--','--check']),('docs',['python3','tools/check_docs.py']),('diff',['git','diff','--check']),('build-default',['cargo','build','--workspace','--locked']),('build-native',['cargo','build','-p','flux-native','--features','system','--locked'])]
rows=[]
for name,command in commands:
 env=dict(os.environ)
 if name=='rustdoc':env['RUSTDOCFLAGS']='-D warnings'
 started=time.monotonic()
 with (out/(name+'.log')).open('wb') as stream:p=subprocess.run(command,stdout=stream,stderr=subprocess.STDOUT,env=env,timeout=240)
 rows.append(dict(name=name,command=command,exit_code=p.returncode,elapsed_seconds=round(time.monotonic()-started,3)))
 (out/'checks.json').write_text(json.dumps(rows,indent=2)+'\n')
 if p.returncode:raise SystemExit(p.returncode)
paths=[Path('target/debug')/n for n in ['fluxwm','fluxwm-config','fluxwm-fixture','fluxwm-native-probe']]
(out/'executables.json').write_text(json.dumps({str(p.resolve()):hashlib.sha256(p.read_bytes()).hexdigest() for p in paths},indent=2)+'\n')
print(json.dumps(dict(ok=True,checks=len(rows))))
