import hashlib,json,shutil,sys
from pathlib import Path
src=Path(sys.argv[1]);out=Path(sys.argv[2]);out.mkdir(parents=True,exist_ok=True)
for p in src.iterdir():
 if p.is_file():shutil.copyfile(p,out/p.name)
x=json.loads((out/'stdout.json').read_text());uses=[];bad=[];frozen=Path(json.loads((out/'snapshot.json').read_text())['frozen'])
for item in x:
 content=item.get('message',{}).get('content',[])
 for c in content if isinstance(content,list) else []:
  if c.get('type')=='tool_use':
   uses.append(c);value=c['input'].get('file_path',c['input'].get('path','.'));p=Path(value);p=p if p.is_absolute() else frozen/p
   if not p.resolve().is_relative_to(frozen.resolve()):bad.append(str(p))
final=[a for a in x if a.get('type')=='result'][-1];manifest=json.loads((out/'input-manifest.json').read_text())
verified=dict(tool_calls=len(uses),tool_names=sorted({u['name'] for u in uses}),tool_uses=uses,all_input_hashes_match=all(hashlib.sha256((frozen/k).read_bytes()).hexdigest()==v for k,v in manifest.items()),input_files=len(manifest),outside_snapshot_requests=bad,result_subtype=final.get('subtype'),is_error=final.get('is_error'),permission_denials=final.get('permission_denials'),model_metadata=list(final.get('modelUsage',{})))
assert not bad and verified['all_input_hashes_match'] and set(verified['tool_names'])<={'Grep','Read'}
verified['usable_review']=bool(uses) and not final.get('is_error') and final.get('subtype')=='success' and not final.get('permission_denials') and not final['result'].startswith('API Error:')
(out/'verified-execution.json').write_text(json.dumps(verified,indent=2)+'\n');(out/('claude-report.md' if verified['usable_review'] else 'failure.txt')).write_text(final['result']+'\n')
print({k:v for k,v in verified.items() if k!='tool_uses'})
