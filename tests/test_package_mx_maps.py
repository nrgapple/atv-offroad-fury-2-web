import sys
import unittest
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'tools'))
from package_mx_maps import VENUES, package

class PackagingTests(unittest.TestCase):
    def test_catalog_identity(self):
        self.assertEqual(len(VENUES), 56)
        self.assertEqual(len(set(v.lower() for v in VENUES)), 56)

    def test_legacy_warnings_require_typed_evidence(self):
        from package_mx_maps import classify_previous_warnings
        previous = [dict(id='mx_fs01', missing=['terrain.ps2.trn: 4..cs', 'Tex/Screen.psm: rendertarget.pst'])]
        current = [dict(id='mx_fs01', dependency_audit=[dict(origin='Tex/Screen.psm', reference='rendertarget.pst', status='dynamic_generated_target')])]
        result = classify_previous_warnings(previous, current)
        self.assertEqual(result[0]['classification'], 'false_positive_binary_token')
        self.assertEqual(result[1]['classification'], 'dynamic_generated_target')

    def test_source_spelling_candidates_are_evidence(self):
        from package_mx_maps import related_source_candidates
        rows = [dict(name='Tex/Nat05_liverystable_detail.pst'), dict(name='Tex/Nat12_pillarA.pst')]
        self.assertEqual(related_source_candidates('nat05_liverystable_detail.dxm.pst', rows), ['Tex/Nat05_liverystable_detail.pst'])
        self.assertEqual(related_source_candidates('nat12 _pillara.pst', rows), ['Tex/Nat12_pillarA.pst'])

    def test_source_count_gate_and_independent_collision_roles(self):
        from package_mx_maps import apply_binding_semantics
        members = {
            'Env/nat01/test.scn': b'[Model1]\nSLT=can01\n[Model2]\nSLT=mailbox\n',
            'Env/nat01/test.toy': b'[PhysicsCollections]\nNumberOfPhysicsCollections=1\n[PhysicsCollection1]\nName=can*\n[PhysicsCollection2]\nName=mailbox\n',
        }
        calls=[]
        def resolve(ref, origin, kind):
            calls.append((ref,kind))
            members['Col/'+ref]=b'collider'
        audit=[dict(origin='Env/nat01/test.scn',reference='can01.col',kind='configuration',status='ambiguous')]
        collections=apply_binding_semantics(members,audit,resolve)
        self.assertEqual(calls,[('can01m.col','actor_whiskers'),('can01f.col','collision_target')])
        self.assertEqual(collections[1]['status'],'inactive_count_gate')
        self.assertEqual(collections[1]['models'],[])
        self.assertEqual(audit[0]['status'],'physics_collection_roles')

    def test_explicit_repairs_do_not_alias_source_colliders(self):
        from package_mx_maps import apply_binding_semantics
        members={'Env/mc09/a.scn':b'[Object1]\nFile=Ring_Trigger.col\nHasCol=False\n', 'Tex/Nat12_pillarA.pst':b'original'}
        audit=[dict(origin='Env/mc09/a.scn',reference='ring_trigger.col',kind='configuration',status='actual_missing'), dict(origin='Env/nat12/a.scn',reference='nat12_sphinx.col',kind='configuration',status='ambiguous'),dict(origin='Tex/Nat12_pillarA.psm',reference='nat12 _pillara.pst',kind='texture',status='actual_missing')]
        apply_binding_semantics(members,audit,lambda *args:None)
        self.assertEqual([e['status'] for e in audit],['nonphysical_marker','source_missing','authored_repair'])
        self.assertFalse(audit[-1]['source_equivalent'])
        self.assertEqual(members['Tex/Nat12_pillarA.pst'],b'original')

    def test_reject_public_output(self):
        with self.assertRaises(ValueError):
            package(Path('/tmp/mx-public-output'))

class PrivateValidation(unittest.TestCase):
    @unittest.skipUnless((Path(__file__).resolve().parents[1] / 'private/reports/mx-map-staging/map-packages.json').exists(), 'run private packaging first')
    def test_all_private_packages(self):
        import json
        import hashlib
        from package_content import Resource
        root = Path(__file__).resolve().parents[1]
        output = root / 'private/reports/mx-map-staging'
        report = json.loads((output / 'map-packages.json').read_text())
        self.assertEqual(len(report), 56)
        for row in report:
            self.assertEqual(row['source_stem'], row['source_stem'].lower())
            self.assertIn('dependency_audit', row)
            for edge in row['dependency_audit']:
                self.assertIn(edge['status'], {'resolved','platform_alias','excluded_shader','ambiguous','actual_missing','dynamic_generated_target','false_positive_inactive_template','source_missing','nonphysical_marker','physics_collection_roles','inactive_count_gate','authored_repair'})
                self.assertFalse(edge['origin'].lower().endswith(('.trn','.pst','.atm','.col')))
            if row['id'] == 'mx_nat10':
                self.assertTrue(all(c['status'] == 'inactive_count_gate' and not c['bindings'] for c in row['physics_collections']))
            if row['id'] in ('mx_nat01','mx_nat06','mx_nat11'):
                bindings=[b for c in row['physics_collections'] for b in c['bindings']]
                self.assertTrue(bindings)
                for binding in bindings:
                    self.assertEqual(set(binding['roles']), {'actor_whiskers','collision_target'})
                    self.assertTrue(all(role['status']=='resolved' for role in binding['roles'].values()))
            data = (output / (row['sha256']+'.bin')).read_bytes()
            self.assertEqual(len(data), row['bytes'])
            self.assertEqual(hashlib.sha256(data).hexdigest(), row['sha256'])
            resource = Resource(data)
            self.assertEqual(len(resource.members), row['member_count'])
            paths = {n for n,_,_ in resource.members}
            for edge in row['dependency_audit']:
                if edge['status'] == 'authored_repair':
                    self.assertIn(edge['repair_target'], paths)
                    self.assertFalse(edge['source_equivalent'])
            for collection in row['physics_collections']:
                if collection['status'] == 'active':
                    for binding in collection['bindings']:
                        for role in binding['roles'].values():
                            if role['status'] == 'resolved':
                                self.assertIn(role['path'], paths)

            self.assertTrue(any(n.lower().endswith('.ps2.trn') for n,_,_ in resource.members))
            self.assertFalse(any(n.lower().endswith(('.vuo','.irx','.elf','.iso')) for n,_,_ in resource.members))
            for i, (_,_,size) in enumerate(resource.members):
                self.assertEqual(len(resource.extract(i)), size)

class ManifestTests(unittest.TestCase):
    def test_additive_atomic_update(self):
        import tempfile, json, hashlib
        from package_mx_maps import integrate_manifest
        with tempfile.TemporaryDirectory() as temporary:
            output=Path(temporary)
            blob=b'test'
            digest=hashlib.sha256(blob).hexdigest()
            (output/(digest+'.bin')).write_bytes(blob)
            existing=dict(path='/MX/BIKE50.RES',lba=10,bytes=4)
            (output/'manifest.json').write_text(json.dumps({'files':[existing]}))
            row=dict(archive='/MX/MAPS/FS01.RES',sha256=digest,bytes=4,member_count=1)
            integrate_manifest(output,[row])
            integrate_manifest(output,[row])
            result=json.loads((output/'manifest.json').read_text())['files']
            self.assertEqual(result[0],existing)
            self.assertEqual(len(result),2)
            self.assertGreater(result[1]['lba'],10)
            self.assertFalse((output/'manifest.json.tmp').exists())

class AdapterTests(unittest.TestCase):
    @unittest.skipUnless((Path(__file__).resolve().parents[1] / 'private/source/ATV Offroad Fury 2 (USA) (v2.00).iso').exists(), 'private source required')
    def test_original_adapter_provenance_and_determinism(self):
        import json, tempfile, hashlib
        from package_mx_maps import package_adapter, ROOT, MX_BUILD
        from package_content import Resource, SOURCE_SHA, config
        with tempfile.TemporaryDirectory(dir=ROOT / 'private/reports') as temporary:
            output=Path(temporary)
            first=package_adapter(output)
            second=package_adapter(output)
            self.assertEqual(first,second)
            from package_mx_maps import integrate_manifest
            retained=[dict(path='/MX/BIKE50.RES',lba=1,bytes=4),dict(path='/MX/MAPS/FS01.RES',lba=3,bytes=4)]
            (output/'manifest.json').write_text(json.dumps(dict(files=retained)))
            integrate_manifest(output,[first])
            integrated=json.loads((output/'manifest.json').read_text())['files']
            self.assertEqual(integrated[:2],retained)
            self.assertEqual(len(integrated),3)
            self.assertEqual(integrated[-1]['source_builds'],first['source_builds'])

            self.assertEqual(first['archive'],'/MX/ADAPTER.RES')
            self.assertEqual(first['source_builds'],dict(atv2_handling=SOURCE_SHA,mx_venues=MX_BUILD))
            resource=Resource((output/(first['sha256']+'.bin')).read_bytes())
            members={n:resource.extract(i) for i,(n,_,_) in enumerate(resource.members)}
            self.assertEqual(set(members), {'FollowCamPresets.pb','tracks/env01/env01.toy','tracks/env02/env02.toy','tracks/nat05/nat05.toy','mx-adapter.json'})
            metadata=json.loads(members['mx-adapter.json'])
            self.assertEqual(metadata['source_builds'],first['source_builds'])
            for entry in metadata['members']:
                self.assertEqual(entry['source_build_sha256'],SOURCE_SHA)
                self.assertEqual(hashlib.sha256(members[entry['path']]).hexdigest(),entry['sha256'])
            for material in metadata['materials'].values():
                if 'zone' in material:
                    zone=config(members[material['path']])['terrainzone'+str(material['zone'])]
                    self.assertIn('surfacefriction',zone)
                    self.assertIn('surfacetraction',zone)

if __name__ == '__main__':
    unittest.main()
