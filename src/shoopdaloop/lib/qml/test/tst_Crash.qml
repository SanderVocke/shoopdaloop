import QtQuick 6.3
import QtTest 1.0
import ShoopDaLoop.PythonBackend

import ShoopConstants
import './testfilename.js' as TestFilename

ShoopTestFile {
    ShoopTestCase {
        name: 'Crash'
        filename : TestFilename.test_filename()

        test_fns: ({
            'test_crash': () => {
                os_utils.test_segfault()
            }
        })
    }
}