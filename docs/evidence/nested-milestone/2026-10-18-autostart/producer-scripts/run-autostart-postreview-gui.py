import hashlib,json,subprocess,time
from pathlib import Path
out=Path('target/autostart-postreview-gui');out.mkdir()
executables=[Path('target/debug')/name for name in ['fluxwm','fluxwm-config','fluxwm-fixture']]
before={str(p.resolve()):hashlib.sha256(p.read_bytes()).hexdigest() for p in executables}
commands=[('autostart','tools/smoke_autostart.py'),('configuration','tools/smoke_config.py'),('widths','tools/smoke_width.py'),('regressions','tools/smoke_private.py'),('focus','tools/smoke_focus.py')]
rows=[]
for name,tool in commands:
 command=['python3',tool,'--bundle','target/configure-verified-xvfb','--artifacts',str(out/name)]
 started=time.monotonic()
 with (out/(name+'.log')).open('wb') as stream:p=subprocess.run(command,stdout=stream,stderr=subprocess.STDOUT,timeout=180)
 rows.append(dict(name=name,command=command,exit_code=p.returncode,elapsed_seconds=round(time.monotonic()-started,3),harness_sha256=hashlib.sha256(Path(tool).read_bytes()).hexdigest()))
 metadata=dict(executables=before,executables_unchanged=all(hashlib.sha256(Path(p).read_bytes()).hexdigest()==h for p,h in before.items()),runs=rows)
 (out/'execution.json').write_text(json.dumps(metadata,indent=2)+'\n')
 if p.returncode:raise SystemExit(p.returncode)
print(json.dumps(dict(ok=metadata['executables_unchanged'],runs=len(rows))))
