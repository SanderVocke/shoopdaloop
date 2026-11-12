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
    cf=media['chunk_frames'];mapping={};samples=[];views=[None]*len(media['views'])
    order=sorted(range(len(views)),key=lambda i:(media['views'][i] is None,
        'range' in (media['views'][i] or {}),-len((media['views'][i] or {}).get('chunks',[]))))
    for i in order:
        view=media['views'][i]
        if view is None:continue
        ids=view['chunks']
        if not ids:views[i]=deepcopy(view);continue
        known=[chunk in mapping for chunk in ids]
        if any(known):
            assert all(known), 'fixture has overlapping non-nested ranges'
            translated=[mapping[chunk] for chunk in ids]
            first=translated[0]%block_chunks
            assert all(chunk%block_chunks==(first+j)%block_chunks for j,chunk in enumerate(translated))
            assert all(translated[j]//block_chunks==translated[j-1]//block_chunks for j in range(1,len(translated)) if (first+j)%block_chunks!=0)
        else:
            start=len(samples)//cf
            samples.extend([0]*(((len(ids)+block_chunks-1)//block_chunks)*block_chunks*cf))
            translated=list(range(start,start+len(ids)))
            for old,new in zip(ids,translated):
                mapping[old]=new;samples[new*cf:(new+1)*cf]=media['samples'][old*cf:(old+1)*cf]
        views[i]={**deepcopy(view),'chunks':translated}
    free=[]
    for _ in range((len(media['free_chunks'])+block_chunks-1)//block_chunks):
        first=len(samples)//cf;free.extend(range(first,first+block_chunks));samples.extend([0]*(block_chunks*cf))
    assert len(samples)//cf<=capacity, 'blocked fixture exceeds physical capacity'
    result=dict(block_chunks=block_chunks,chunk_frames=cf,chunk_limit=capacity,samples=samples,free_chunks=free,views=views)
    return canonicalize(result)
