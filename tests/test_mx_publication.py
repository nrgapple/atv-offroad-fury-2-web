"""Publication-boundary tests; no uploads and no generated runtime acceptance."""
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'tools'))
from package_content import ROOT, SOURCE_SHA, MX_SOURCE_SHA, Resource, pack_resource, validate_mx_resource, preserve_mx_records, validated_content
from promote_mx_content import validate_acceptance, staged_records


class ValidationTests(unittest.TestCase):
    def record(self, members):
        payload=pack_resource(members)
        return dict(path='/MX/MAPS/FS01.RES',source_build_sha256=MX_SOURCE_SHA,repacked=True,member_count=len(members)),payload

    def test_unknown_ids_and_source_labels_fail(self):
        row,payload=self.record([('Env/FS01/FS01.scn',b'fixture')])
        for path in ['/MX/MAPS/FS09.RES','/MX/MAPS/NAT18.RES','/MX/MAPS/SX19.RES','/MX/MAPS/fs01.RES','/MX/OTHER.RES']:
            with self.assertRaises(ValueError):
                validate_mx_resource(dict(row,path=path),payload)
        with self.assertRaisesRegex(ValueError,'provenance'):
            validate_mx_resource(dict(row,source_build_sha256=SOURCE_SHA),payload)

    def test_invalid_members_and_forged_payload_fail(self):
        required=[('Env/FS01/'+name,b'fixture') for name in ['FS01.scn','FS01.toy','FS01.ps2.trn','FS01_d.pst','TerrainPlain.psm','TerrainLOD.psm']]
        for name in ['Env/FS01/boot.elf','Env/FS01/shader.vuo','Env/FS01/raw.bin','Env/FS01/game.iso','Env/FS01/map_mst.res','private/key.psm','Env/FS01/private/key.psm','Env/FS02/foreign.pst','Tex/.private.pst','Env/FS01/other.vut','Tex/uninspected.pst']:
            row,payload=self.record(required+[(name,b'fixture')])
            with self.assertRaises(ValueError):
                validate_mx_resource(row,payload)
        row,payload=self.record(required+[('env/fs01/fs01.scn',b'duplicate')])
        with self.assertRaisesRegex(ValueError,'inventory'):
            validate_mx_resource(row,payload)

    def test_adapter_requires_both_builds_and_exact_member_set(self):
        payload=pack_resource([('FollowCamPresets.pb',b'fixture')])
        row=dict(path='/MX/ADAPTER.RES',source_build_sha256=SOURCE_SHA,repacked=True,member_count=1)
        with self.assertRaisesRegex(ValueError,'source builds'):
            validate_mx_resource(row,payload)
        row['source_builds']=dict(atv2_handling=SOURCE_SHA,mx_venues=MX_SOURCE_SHA)
        with self.assertRaisesRegex(ValueError,'adapter member'):
            validate_mx_resource(row,payload)

    def test_acceptance_schema_hashes_vehicles_and_counts(self):
        # Synthetic schema fixtures only. Never persisted or passed to promote.
        maps=[dict(id='fixture_'+str(i),sha256=hashlib.sha256(str(i).encode()).hexdigest()) for i in range(56)]
        adapter=dict(sha256='f'*64)
        good=dict(version=1,adapter_sha256=adapter['sha256'],maps=[dict(**r,atv=True,motorcycle=True,steps_each=360,unsupported_models=0) for r in maps])
        validate_acceptance(good,maps,adapter)
        for field,value in [('sha256','0'*64),('atv',False),('motorcycle',False),('steps_each',359),('steps_each',True),('unsupported_models',1),('unsupported_models',False)]:
            bad=json.loads(json.dumps(good));bad['maps'][0][field]=value
            with self.assertRaises(ValueError):validate_acceptance(bad,maps,adapter)
        for bad in [dict(good,maps=good['maps'][:-1]),dict(good,maps=good['maps'][:-1]+[good['maps'][0]]),dict(good,adapter_sha256='0'*64)]:
            with self.assertRaises(ValueError):validate_acceptance(bad,maps,adapter)


class PrivateSourceValidationTests(unittest.TestCase):
    @unittest.skipUnless((ROOT/'private/reports/mx-map-staging/adapter-package.json').exists(),'private staging required')
    def test_all_real_packages_and_base_regeneration_preservation(self):
        import os
        staging=ROOT/'private/reports/mx-map-staging'
        maps,adapter,records=staged_records(staging)
        self.assertEqual(len(maps),56)
        self.assertEqual(len(records),57)
        with tempfile.TemporaryDirectory(dir=ROOT/'private/reports') as temporary:
            output=Path(temporary)
            base_blob=b'base fixture'
            digest=hashlib.sha256(base_blob).hexdigest()
            (output/(digest+'.bin')).write_bytes(base_blob)
            base=dict(path='/TRACKS/TRACKSA/ENV01/E01_FR.RES',lba=0,bytes=len(base_blob),sha256=digest,url='/content/'+digest+'.bin',directory=False)
            sector=1
            prior=[]
            for row in records:
                os.link(staging/(row['sha256']+'.bin'),output/(row['sha256']+'.bin'))
                prior.append(dict(row,lba=sector))
                sector+=(row['bytes']+2047)//2048+1
            manifest=dict(version=1,source_build_sha256=SOURCE_SHA,files=[base]+prior)
            (output/'manifest.json').write_text(json.dumps(manifest))
            self.assertEqual(len(validated_content(output)),59)
            newer=dict(manifest,files=[dict(base,lba=999999)])
            combined=preserve_mx_records(output,newer)
            self.assertEqual({r['path'] for r in combined['files']},{r['path'] for r in manifest['files']})
            for old,new in zip(prior,combined['files'][1:]):
                self.assertEqual({k:v for k,v in new.items() if k!='lba'},{k:v for k,v in old.items() if k!='lba'})
                self.assertGreater(new['lba'],999999)
            (output/'manifest.json').write_text(json.dumps(combined))
            validated_content(output)
            overlap=dict(combined,files=[dict(r) for r in combined['files']])
            overlap['files'][1]['lba']=overlap['files'][0]['lba']
            (output/'manifest.json').write_text(json.dumps(overlap))
            with self.assertRaisesRegex(ValueError,'overlapping'):
                validated_content(output)
            (output/'manifest.json').write_text(json.dumps(combined))
            # Extra private/unreferenced artifacts never join the allowed set.
            (output/'private-secret.txt').write_text('fixture')
            self.assertNotIn(output/'private-secret.txt',validated_content(output))
            duplicate=dict(combined,files=combined['files']+[combined['files'][0]])
            (output/'manifest.json').write_text(json.dumps(duplicate))
            with self.assertRaisesRegex(ValueError,'duplicate'):
                validated_content(output)


if __name__=='__main__':
    unittest.main()
