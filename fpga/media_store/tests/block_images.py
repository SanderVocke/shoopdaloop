"""Fixture/snapshot conversion only; runtime allocation stays in Rust and RTL."""
from copy import deepcopy


def view_blocks(view, chunk_frames, block_chunks):
    ids=view['chunks']
    first=ids[0]%block_chunks if ids else 0
    blocks=[chunk//block_chunks for i,chunk in enumerate(ids) if i==0 or (first+i)%block_chunks==0]
    offset=first*chunk_frames+view.get('range',{}).get('offset',0)
    return blocks,offset


def canonicalize(media):
    raw=media['samples'];samples=[0]*len(raw);cf=media['chunk_frames']
    for view in media['views']:
        if view is None:continue
        offset=view.get('range',{}).get('offset',0)
        remaining=view['frames']
        for chunk in view['chunks']:
            count=min(cf-offset,remaining);start=chunk*cf+offset
            samples[start:start+count]=raw[start:start+count]
            remaining-=count;offset=0
        assert remaining==0
    media['samples']=samples
    return media


def reblock(media, block_chunks, capacity):
    if media.get('block_chunks',1)==block_chunks:return deepcopy(media)
    assert media.get('block_chunks',1)==1
    cf=media['chunk_frames'];mapping={};samples=[];views=[]
    following={};previous={};chunks=set()
    for view in media['views']:
        if view is None:continue
        ids=view['chunks'];chunks.update(ids)
        for left,right in zip(ids,ids[1:]):
            assert following.setdefault(left,right)==right,'fixture has branching chunk sequences'
            assert previous.setdefault(right,left)==left,'fixture has merging chunk sequences'
    for head in sorted(chunks-previous.keys()):
        sequence=[head]
        while sequence[-1] in following:
            sequence.append(following[sequence[-1]])
            assert len(sequence)<=len(chunks),'fixture has cyclic chunk sequences'
        start=len(samples)//cf
        samples.extend([0]*(((len(sequence)+block_chunks-1)//block_chunks)*block_chunks*cf))
        for offset,old in enumerate(sequence):
            new=start+offset;mapping[old]=new
            samples[new*cf:(new+1)*cf]=media['samples'][old*cf:(old+1)*cf]
    assert len(mapping)==len(chunks),'fixture has cyclic chunk sequences'
    for view in media['views']:
        views.append(None if view is None else {**deepcopy(view),'chunks':[mapping[chunk] for chunk in view['chunks']]})
    free=[]
    for _ in range((len(media['free_chunks'])+block_chunks-1)//block_chunks):
        first=len(samples)//cf;free.extend(range(first,first+block_chunks));samples.extend([0]*(block_chunks*cf))
    assert len(samples)//cf<=capacity, 'blocked fixture exceeds physical capacity'
    result=dict(block_chunks=block_chunks,chunk_frames=cf,chunk_limit=capacity,samples=samples,free_chunks=free,views=views)
    return canonicalize(result)
