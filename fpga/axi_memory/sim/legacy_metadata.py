"""Scalar backing-memory port for the legacy functional diagnostic."""


class LegacyMetadata:
    def __init__(self, owner):
        self.owner=owner
        self.pending=None

    def drive(self):
        owner=self.owner
        item=self.pending
        owner.set(memory_meta_ready=int(item is None and not getattr(owner,'pause',False)),
            memory_meta_response_valid=int(item is not None and item[0]==0 and not getattr(owner,'pause_responses',False)),
            memory_meta_read_data=item[1] if item else 0,
            memory_meta_response_error=int(item is not None and item[2]),
            memory_meta_write_ready=0,memory_meta_read_valid=0)

    def sample(self):
        dut=self.owner.dut
        return {name:int(getattr(dut,'memory_meta_'+name).value) for name in
            ('valid','ready','write','address','write_data','response_valid','response_ready')}

    def step(self, packet, reset):
        owner=self.owner
        if reset:self.pending=None;return
        if packet['response_valid'] and packet['response_ready']:self.pending=None
        elif self.pending is not None and self.pending[0]>0:self.pending[0]-=1
        if packet['valid'] and packet['ready']:
            assert self.pending is None
            address=packet['address']-owner.base
            assert address%4==0 and 0<=address<len(owner.memory)*4
            index=address//4
            error=owner.fail_next
            owner.fail_next=False
            if packet['write'] and not error:owner.memory[index]=packet['write_data']
            self.pending=[owner.rng.randrange(4) if owner.stalls else 0,owner.memory[index],error]
