#!/usr/bin/env python3
"""Promote accepted private MX staging packages locally; never uploads content."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import tempfile

from package_content import ROOT, MAX_FILE, MX_SOURCE_SHA, mx_map_hashes, validate_mx_resource, validated_content
from release_checkpoint import publication_lock


def read_json(path, maximum=1024*1024):
    if path.is_symlink() or not 0 < path.stat().st_size <= maximum:
        raise ValueError('invalid MX JSON input')
    return json.loads(path.read_text())


def validate_acceptance(acceptance, maps, adapter):
    """Validation input emitted by the actual two-vehicle native venue sweep."""
    if type(acceptance.get('version')) is not int or acceptance['version'] != 1 or acceptance.get('adapter_sha256') != adapter['sha256']:
        raise ValueError('acceptance adapter/version mismatch')
    rows=acceptance.get('maps')
    if not isinstance(rows,list) or len(rows) != 56:
        raise ValueError('acceptance requires all 56 maps')
    expected={row['id']:row['sha256'] for row in maps}
    seen=set()
    for row in rows:
        key=row.get('id')
        if key in seen or key not in expected or row.get('sha256') != expected[key]:
            raise ValueError('acceptance map ID/hash mismatch')
        if row.get('atv') is not True or row.get('motorcycle') is not True or type(row.get('steps_each')) is not int or row['steps_each'] < 360 or type(row.get('unsupported_models')) is not int or row['unsupported_models'] != 0:
            raise ValueError('acceptance requires both vehicles, 360 steps, zero unsupported models')
        seen.add(key)
    if seen != set(expected):
        raise ValueError('incomplete acceptance map set')


def atomic_write(path, payload):
    if path.is_symlink():
        raise ValueError('refuse symlink output')
    temporary=None
    try:
        with tempfile.NamedTemporaryFile(dir=path.parent,delete=False) as stream:
            temporary=Path(stream.name)
            stream.write(payload)
            stream.flush()
            os.fsync(stream.fileno())
        temporary.replace(path)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def staged_records(staging):
    maps=read_json(staging/'map-packages.json',32*1024*1024)
    adapter=read_json(staging/'adapter-package.json')
    expected=mx_map_hashes()
    if len(maps)!=56 or {row['archive'] for row in maps} != set(expected) or any(row['id']!='mx_'+row['archive'].split('/')[-1][:-4].lower() for row in maps):
        raise ValueError('staging requires exactly 56 known venue records')
    records=[]
    for row in maps+[adapter]:
        digest=row['sha256']
        if len(digest)!=64 or any(c not in '0123456789abcdef' for c in digest):
            raise ValueError('invalid staging hash')
        blob=staging/(digest+'.bin')
        if blob.is_symlink() or not 0 < blob.stat().st_size <= MAX_FILE:
            raise ValueError('invalid staging blob')
        payload=blob.read_bytes()
        if len(payload)!=row['bytes'] or hashlib.sha256(payload).hexdigest()!=digest:
            raise ValueError('staging blob digest mismatch')
        record=dict(path=row['archive'],sha256=digest,bytes=row['bytes'],directory=False,lazy=True,repacked=True,source_build_sha256=row.get('source_build_sha256',MX_SOURCE_SHA),member_count=row['member_count'],url='/content/'+digest+'.bin')
        if 'source_builds' in row:
            record['source_builds']=row['source_builds']
        validate_mx_resource(record,payload)
        records.append(record)
    if records[-1]['path']!='/MX/ADAPTER.RES':
        raise ValueError('staging adapter path mismatch')
    return maps,adapter,records


def promote(staging, output, acceptance_path):
    staging,output=staging.resolve(),output.resolve()
    private=(ROOT/'private').resolve()
    if not staging.is_relative_to(private) or not output.is_relative_to(private) or staging==output:
        raise ValueError('promotion requires distinct ignored private directories')
    maps,adapter,records=staged_records(staging)
    validate_acceptance(read_json(acceptance_path),maps,adapter)
    # Browser acceptance is an additional parent prerequisite to invoking this
    # command. This tool neither runs that audit nor manufactures its result.
    with publication_lock(ROOT):
        validated_content(output)
        manifest=read_json(output/'manifest.json')
        replacing={r['path'] for r in records}
        retained=[r for r in manifest['files'] if r['path'] not in replacing]
        sector=max(((r['lba']*2048+r['bytes']+2047)//2048 for r in retained),default=0)+1
        for row in records:
            payload=(staging/(row['sha256']+'.bin')).read_bytes()
            if hashlib.sha256(payload).hexdigest()!=row['sha256']:
                raise ValueError('staging changed during promotion')
            atomic_write(output/(row['sha256']+'.bin'),payload)
            retained.append(dict(row,lba=sector))
            sector+=(row['bytes']+2047)//2048+1
        manifest['files']=retained
        atomic_write(output/'manifest.json',(json.dumps(manifest,indent=2)+'\n').encode())
    return dict(archives=len(records),bytes=sum(r['bytes'] for r in records))


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--staging',type=Path,default=ROOT/'private/reports/mx-map-staging')
    parser.add_argument('--output',type=Path,default=ROOT/'private/reports/hosted-content')
    parser.add_argument('--acceptance',type=Path,required=True,help='actual all-56 two-vehicle native acceptance JSON; browser acceptance also required before invocation')
    args=parser.parse_args()
    print(json.dumps(promote(args.staging,args.output,args.acceptance)))
