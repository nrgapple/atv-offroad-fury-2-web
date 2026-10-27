"""Synthetic dependency checks for the separate, Site-only content package."""
import struct
import unittest

from package_content import Resource, compact_map_placements, compact_rider_materials, pack_resource, selected_train_names, selected_body_impact_names, validate_motorcycle_resource, MX_SOURCE_SHA


def mesh(material):
    return (b'psg\0' + struct.pack('<II', 3, 1) + bytes(224)
            + struct.pack('<I', 1) + material.encode().ljust(64, b'\0'))


class ContentTests(unittest.TestCase):
    def test_race_variant_keeps_only_members_beside_the_national_race(self):
        from package_content import compact_race_variant
        national = pack_resource([('tracks/env05/env05.tr2', b'terrain'), ('geometry/gate.psg', b'nat')])
        variant = pack_resource([('tracks/env05/env05.tr2', b'terrain'), ('geometry/gate.psg', b'end'),
                                 ('tracks/env05/env05endsE.tdf', b'route')])
        kept = Resource(compact_race_variant(variant, national))
        self.assertEqual([name for name, _, _ in kept.members], ['geometry/gate.psg', 'tracks/env05/env05endsE.tdf'])
        placements = Resource(compact_map_placements(pack_resource([('tracks/env05/env05.toy', b'toy'),
            ('tracks/env05/env05endlE.tdf', b'route')]), 'ENV05', national))
        self.assertEqual(len(placements.members), 2)
        with self.assertRaises(ValueError):
            compact_race_variant(national, national)

    def test_motorcycle_package_rejects_executable_and_wrong_source(self):
        record = {'source_build_sha256': MX_SOURCE_SHA, 'repacked': True, 'member_count': 1}
        validate_motorcycle_resource(record, pack_resource([('Veh/Anim/authored.psg', b'model')]))
        for name in ['GAME_REL.BIN', 'Veh/Anim/boot.elf', 'Rider/../source.mot']:
            with self.assertRaises(ValueError):
                validate_motorcycle_resource(record, pack_resource([(name, b'bytes')]))
        with self.assertRaisesRegex(ValueError, 'source build'):
            validate_motorcycle_resource(dict(record, source_build_sha256='0'*64), pack_resource([('Veh/test.phy', b'config')]))

    def test_body_impact_selects_required_bank_only_and_rejects_invalid_paths(self):
        table = b'''[BodyImpact]
NumSounds=2
Sound_1=audio\\rider\\authored1.vag
Sound_2=rider/authored2.vag
[Unused]
NumSounds=1
Sound_1=unused.vag
'''
        self.assertEqual(selected_body_impact_names(table), {'rider/authored1.vag', 'rider/authored2.vag'})
        with self.assertRaisesRegex(ValueError, 'sample path'):
            selected_body_impact_names(table.replace(b'audio\\rider\\authored1.vag', b'../private.vag'))
        with self.assertRaisesRegex(ValueError, 'duplicated'):
            selected_body_impact_names(table.replace(b'rider/authored2.vag', b'rider/authored1.vag'))

    def test_train_selects_only_bound_procedural_body_audio_and_deduplicates(self):
        toy = b'''[Animations]
NumberOfAnimations=3
[Animation1]
AnimationType=Procedural
VUE=locomotive.Vue
NumberOfLoopingSounds=1
[Animation1LoopSound1]
AttachToObject=Body
SoundResourceFile=audio\\soultree\\test
[Animation2]
AnimationType=Procedural
VUE=locomotive.vue
NumberOfLoopingSounds=1
[Animation2LoopSound1]
AttachToObject=Body
SoundResourceFile=soultree/test.VAG
[Animation3]
AnimationType=Procedural
VUE=other.vue
NumberOfLoopingSounds=1
[Animation3LoopSound1]
AttachToObject=Body
SoundResourceFile=unused
'''
        self.assertEqual(selected_train_names(toy), {'soultree/test.vag'})
        with self.assertRaisesRegex(ValueError, 'sample path'):
            selected_train_names(toy.replace(b'audio\\soultree\\test', b'../private'))
        with self.assertRaisesRegex(ValueError, 'attachment'):
            selected_train_names(toy.replace(b'AttachToObject=Body', b'AttachToObject=Wheel', 1))

    def test_rider_retains_all_passes_and_default_bindings(self):
        meshes = pack_resource([('body.psg', mesh('shirt')), ('arm.psg', mesh('hand'))])
        members = [('shirt.psm', b'[ShaderPass_1]\nTextureMap_1=cloth\n[ShaderPass_2]\nTextureMap_1=dirt'),
                   ('cloth.pst', b'cloth'), ('dirt.pst', b'dirt'), ('hand.pst', b'hand'),
                   ('unused.pst', b'unused')]
        result = Resource(compact_rider_materials(meshes, pack_resource(members)))
        self.assertEqual([(name, result.extract(i)) for i, (name, _, _) in enumerate(result.members)], members[:-1])

    def test_missing_texture_fails_instead_of_shipping_incomplete_material(self):
        with self.assertRaisesRegex(ValueError, 'missing referenced rider texture'):
            compact_rider_materials(pack_resource([('body.psg', mesh('shirt'))]),
                                    pack_resource([('shirt.psm', b'[ShaderPass_1]\nTextureMap_1=absent')]))

    def test_placements_select_exact_common_name(self):
        archive = pack_resource([('map/env01endl.toy', b'wrong'), ('map/env01.toy', b'correct'), ('unused.pst', b'unused')])
        result = Resource(compact_map_placements(archive, 'ENV01'))
        self.assertEqual(len(result.members), 1)
        self.assertEqual(result.named('env01.toy'), b'correct')
        with self.assertRaisesRegex(ValueError, 'expected one'):
            compact_map_placements(archive, 'ENV02')

class PlayerContentTests(unittest.TestCase):
    def test_quad_package_contains_only_live_geometry_physics_and_collision(self):
        from package_content import compact_player_archive, pack_resource, Resource
        names = ['quad/atv26.psg', 'quad/ATV26S01.phy', 'geometry/ATV26ao.col', 'geometry/ATV26ar.col', 'particles/quaddust.emt', 'particles/quaddirtchunk.emt', 'particles/quaddirtspray.emt']
        source = pack_resource([(name, b'fixture') for name in names] + [('Quad/ATV26.Vut', b'unused'), ('quad/ATV26.mcf', b'unused'), ('geometry/ATV26f.col', b'unused')])
        result = Resource(compact_player_archive(source, '/QUAD/BIKESB/ATV26.RES'))
        self.assertEqual([m[0] for m in result.members], names)
        self.assertTrue(all(result.extract(i) == b'fixture' for i in range(7)))
        with self.assertRaisesRegex(ValueError, 'missing or duplicated'):
            compact_player_archive(pack_resource([(names[0], b'fixture')]), '/QUAD/BIKESB/ATV26.RES')
        duplicate = [(name, b'fixture') for name in names[:-1]] + [('second/atv26.psg', b'fixture')]
        with self.assertRaisesRegex(ValueError, 'missing or duplicated'):
            compact_player_archive(pack_resource(duplicate), '/QUAD/BIKESB/ATV26.RES')

    def test_rider_package_excludes_first_person_and_unbound_physics(self):
        from package_content import compact_player_archive, pack_resource, Resource
        source = pack_resource([('anim/rider/frider.psg', b'mesh'), ('anim/Rider/frider.mcf', b'catalog'), ('anim/Rider/farms.psg', b'unused'), ('frider.phy', b'unused')])
        result = Resource(compact_player_archive(source, '/RIDER/FRIDER.RES'))
        self.assertEqual([m[0] for m in result.members], ['anim/rider/frider.psg', 'anim/Rider/frider.mcf'])

    def test_motion_package_keeps_catalog_union_and_drops_unused_arm_clips(self):
        from package_content import compact_rider_motions, pack_resource, Resource
        catalog = b'[General info]\nNumberOfMotions=2\n[Motion 1]\nVUE=anim\\rider\\ride.vue\n[Motion 2]\nVUE=jump.MOT\n'
        other = b'[General info]\nNumberOfMotions=1\n[Motion 1]\nVUE=RIDE.MOT\n'
        source = pack_resource([('anim/ride.mot', b'ride'), ('anim/jump.mot', b'jump'), ('anim/arms.mot', b'unused'), ('riderarmleft.mcf', b'unused')])
        result = Resource(compact_rider_motions(source, [catalog, other]))
        self.assertEqual([m[0] for m in result.members], ['anim/ride.mot', 'anim/jump.mot'])
        with self.assertRaisesRegex(ValueError, 'missing or duplicated'):
            compact_rider_motions(source, [other.replace(b'RIDE', b'missing')])
        duplicate = pack_resource([('first/ride.mot', b'one'), ('second/ride.mot', b'two')])
        with self.assertRaisesRegex(ValueError, 'missing or duplicated'):
            compact_rider_motions(duplicate, [catalog])

    def test_vancouver_thumbnail_includes_only_two_required_halves(self):
        from package_content import compact_vancouver_thumbnail, pack_resource, Resource
        source = pack_resource([('ui/trk_free05_a.pst', b'a'), ('ui/trk_free05_b.pst', b'b'), ('ui/trk_free01_a.pst', b'unrelated')])
        result = Resource(compact_vancouver_thumbnail(source))
        self.assertEqual([x[0] for x in result.members], ['ui/trk_free05_a.pst', 'ui/trk_free05_b.pst'])
        with self.assertRaisesRegex(ValueError, 'missing or duplicated'):
            compact_vancouver_thumbnail(pack_resource([('ui/trk_free05_a.pst', b'a')]))

    def test_menu_rider_package_excludes_unrelated_page_and_executable_members(self):
        from package_content import compact_menu_riders, pack_resource, Resource
        names = ['mrider_ui.psg', 'frider_ui.psg']
        for stem, count in [('mrider', 7), ('frider', 5)]:
            names += [f'{stem}_body_breathe.mot'] + [f'{stem}_body_idle{i:02}.mot' for i in range(1, count + 1)]
        source = pack_resource([(f'ui/anim/{n}', b'fixture') for n in names] + [('unused.vut', b'private'), ('other.pst', b'unrelated')])
        textures = ['atv2_helmet_ui', 'atv2_shirt_ui', 'atv2_black_gloves_ui', 'atv2_black_boots_ui', 'atv2_black_goggles_ui', 'atv2_pants_ui', 'atv2_sleeves_ui']
        materials = pack_resource([(n + '.pst', b'fixture') for n in textures] + [('unused.pst', b'unrelated')])
        result = Resource(compact_menu_riders(source, materials, b'config'))
        self.assertEqual(len(result.members), len(names) + len(textures) + 1)
        self.assertFalse(any('unused' in n or 'other' in n or n.endswith('.vut') for n, _, _ in result.members))
        with self.assertRaisesRegex(ValueError, 'missing or duplicated'):
            compact_menu_riders(pack_resource([('mrider_ui.psg', b'fixture')]), materials, b'config')


if __name__ == '__main__':
    unittest.main()
