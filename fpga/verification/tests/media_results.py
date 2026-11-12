"""Normalize exported views to logical session media."""
def logical_media(snapshot,views,states):
    result=[]
    for row in views:
        state=next(s for s in states if s['id']==row['id'])
        if state['mode']=='recording':continue
        channels=[]
        for view in row['views']:
            value=snapshot['views'][view]
            data=[]
            for chunk in value['chunks']:
                first=chunk*snapshot['chunk_frames'];data.extend(snapshot['samples'][first:first+snapshot['chunk_frames']])
            offset=value.get('range',{}).get('offset',0)
            channels.append(data[offset:offset+value['frames']])
        if channels and channels[0]:
            result.append(dict(name=f"loop:{row['id']['slot']}:{row['id']['generation']}",frames=len(channels[0]),samples=dict(encoding='q824',channels=channels)))
    return result
