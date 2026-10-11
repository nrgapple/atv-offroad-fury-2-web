"""Synthetic dependency checks for the separate, Site-only content package."""
import struct
import unittest

from package_content import Resource, compact_map_placements, compact_rider_materials, pack_resource


def mesh(material):
    return (b'psg\0' + struct.pack('<II', 3, 1) + bytes(224)
            + struct.pack('<I', 1) + material.encode().ljust(64, b'\0'))


class ContentTests(unittest.TestCase):
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


if __name__ == '__main__':
    unittest.main()
