"""Fixture packing preserves logical samples and overlapping ownership."""
from copy import deepcopy
from pathlib import Path
import sys
import unittest

sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'fpga/media_store/tests'))
from block_images import reblock,view_blocks


def samples(media,view):
    data=[];cf=media['chunk_frames']
    for chunk in view['chunks']:data.extend(media['samples'][chunk*cf:(chunk+1)*cf])
    offset=view.get('range',{}).get('offset',0)
    return data[offset:offset+view['frames']]


class BlockImages(unittest.TestCase):
    def test_overlapping_ranges_share_one_packed_sequence(self):
        media=dict(chunk_frames=2,samples=list(range(10,18)),free_chunks=[3],views=[
            dict(chunks=[0,1],frames=3,range=dict(offset=1)),
            dict(chunks=[1,2],frames=4,range=dict(offset=0))])
        before=deepcopy(media)
        result=reblock(media,2,8)
        self.assertEqual(media,before)
        for old,new in zip(media['views'],result['views']):
            self.assertEqual(samples(media,old),samples(result,new))
        self.assertEqual(result['views'][0]['chunks'][1],result['views'][1]['chunks'][0])
        self.assertEqual(view_blocks(result['views'][1],2,2),([0,1],2))
        self.assertEqual(result['free_chunks'],[4,5])
        self.assertEqual(result['samples'][0],0)

    def test_independent_writers_reserve_separate_blocks(self):
        media=dict(chunk_frames=2,samples=[1,2,3,4],free_chunks=[],views=[
            dict(chunks=[0],frames=2),dict(chunks=[1],frames=2)])
        result=reblock(media,32,64)
        self.assertEqual([v['chunks'] for v in result['views']],[[0],[32]])
        self.assertEqual(samples(result,result['views'][1]),[3,4])
        with self.assertRaisesRegex(AssertionError,'capacity'):reblock(media,32,32)

    def test_unrepresentable_branching_and_cycles_are_rejected(self):
        media=dict(chunk_frames=1,samples=[1,2,3],free_chunks=[],views=[
            dict(chunks=[0,1],frames=2),dict(chunks=[0,2],frames=2)])
        with self.assertRaisesRegex(AssertionError,'branching'):reblock(media,2,8)
        media['views']=[dict(chunks=[0,1,0],frames=3)]
        with self.assertRaisesRegex(AssertionError,'cyclic'):reblock(media,2,8)
