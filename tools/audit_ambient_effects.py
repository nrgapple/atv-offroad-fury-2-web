#!/usr/bin/env python3
"""Inspect original emitters and actor declarations; output stays private."""
import argparse,json
from pathlib import Path
from package_content import Resource,config
ROOT=Path(__file__).resolve().parents[1]
def audit():
    source=json.loads((ROOT/'private/reports/import.json').read_text())
    iso=ROOT/'private/source'/Path(source['source_archive']).with_suffix('.iso').name
    files=json.loads((ROOT/'private/reports/disc-files.json').read_text());maps={}
    with iso.open('rb') as stream:
        for entry in files:
            path=entry['path']
            if entry['directory'] or not path.startswith('/TRACKS/') or not path.endswith('.RES'):continue
            family=path.split('/')[-2]
            if not ('_FR.' in path or (family=='GAME01' and '_SO.' in path) or (family=='TRAINING' and '_TR.' in path)):continue
            stream.seek(entry['lba']*2048);resource=Resource(stream.read(entry['bytes']))
            record=maps.setdefault(family,{'archives':[],'toys':{},'emitters':{},'animations':{}});record['archives'].append(path)
            names={Path(name.replace(chr(92),'/')).name.lower():i for i,(name,*_) in enumerate(resource.members)}
            for name,index in names.items():
                if not name.endswith('.toy'):continue
                document=config(resource.extract(index));count=0
                for section,p in document.items():
                    if section.startswith('particleemitter') and section!='particleemitters':
                        emt=p.get('emitterfilename','').lower();parameters=config(resource.extract(names[emt])) if emt in names else None
                        key=name+':'+section;record['emitters'][key]={'toy':name,'section':section,'config':p,'parameters':parameters};count+=1
                    if section.startswith('animation') and section!='animations':record['animations'][name+':'+section]={'toy':name,'section':section,'config':p}
                record['toys'][name]={'declared':document.get('particleemitters',{}).get('numberofemitters','0'),'records':count}
    result=[]
    for name,r in sorted(maps.items()):
        r['emitters']=list(r['emitters'].values());r['animations']=list(r['animations'].values());result.append({'map':name,**r})
    return {'scope':'FR/TR/SO TOY and EMT declarations; distinct common and mode sidecars retained; not active runtime proof','maps':result}
def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--output',type=Path,default=ROOT/'private/reports/ambient-particles/all-venues.json');args=p.parse_args()
    if not args.output.resolve().is_relative_to((ROOT/'private').resolve()):p.error('output must stay private')
    report=audit();args.output.parent.mkdir(parents=True,exist_ok=True);args.output.write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({'venues':len(report['maps']),'source_emitters':sum(len(m['emitters'])for m in report['maps']),'source_animations':sum(len(m['animations'])for m in report['maps'])}))
    for m in report['maps']:
        if m['emitters']:print(json.dumps({'map':m['map'],'emitters':len(m['emitters']),'types':sorted(set(e['config'].get('typeid','0')for e in m['emitters'])),'actors':len(m['animations'])}))
if __name__=='__main__':main()
