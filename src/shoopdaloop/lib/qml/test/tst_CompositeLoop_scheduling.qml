import QtQuick 6.6

import ShoopConstants
import './testfilename.js' as TestFilename
import '..'

ShoopTestFile {
    id: root

    // UTILITIES
    component FakeLoop: Item {
        id: fakeloop
        property string obj_id: 'unknown'
        property int length: 100
        property int n_cycles: 1
        property int position: 0

        property var maybe_loop: this
        
        signal cycled(int cycle_nr)

        RegisterInRegistry {
            object: fakeloop
            key: fakeloop.obj_id
            registry: registries.objects_registry
        }
    }

    component FakeSyncLoop: FakeLoop {
        id: fakesync

        RegisterInRegistry {
            object: fakesync
            registry: registries.state_registry
            key: 'sync_loop'
        }
    }

    function loops_to_obj_ids(schedule) {
        var rval = new Object()
        for (let key in schedule) {
            rval[key] = new Object()
            let keys = ['loops_start', 'loops_end', 'loops_ignored']
            keys.forEach((k) => {
                rval[key][k] = []
                if (k in schedule[key]) {
                    if (k == 'loops_start') {
                        schedule[key][k].forEach(v => rval[key][k].push([v[0].obj_id, v[1]]))
                    } else {
                        schedule[key][k].forEach(v => rval[key][k].push(v.obj_id))
                    }
                }
            })
        }
        return rval
    }
    // END UTILITIES


    // TESTCASES

    // Sequential loop scheduling
    Item {
        FakeSyncLoop {
            id: sequential_sched_sync_loop
        }
        FakeLoop {
            id: sequential_sched_1
            obj_id: 'sequential_sched_1'
            n_cycles: 2
        }
        FakeLoop {
            id: sequential_sched_2
            obj_id: 'sequential_sched_2'
            n_cycles: 3
        }

        CompositeLoop {
            id: sequential_sched_lut

            ShoopTestCase {
                name: 'CompositeLoop_sequential_sched'
                filename: TestFilename.test_filename()
                when: sequential_sched_lut.sync_length

                test_fns: ({
                    'test_sequential_schedule': () => {
                        sequential_sched_lut.add_loop(sequential_sched_1, 0, undefined, 0)
                        sequential_sched_lut.add_loop(sequential_sched_2, 1, undefined, 0)

                        verify_eq(
                            root.loops_to_obj_ids(sequential_sched_lut.schedule),
                            {
                                0: {
                                    'loops_start': [['sequential_sched_1', null]],
                                    'loops_end': [],
                                    'loops_ignored': []
                                },
                                2: {
                                    'loops_start': [],
                                    'loops_end': ['sequential_sched_1'],
                                    'loops_ignored': []
                                },
                                3: {
                                    'loops_start': [['sequential_sched_2', null]],
                                    'loops_end': [],
                                    'loops_ignored': []
                                },
                                6: {
                                    'loops_start': [],
                                    'loops_end': ['sequential_sched_2'],
                                    'loops_ignored': []
                                },
                            }
                        )
                    },
                })
            }
        }
    }
}