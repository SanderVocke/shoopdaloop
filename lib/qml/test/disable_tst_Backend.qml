import QtQuick 2.3
import QtTest 1.0

TestCaseWithBackend {
    function test_backend() {
        verify(backend.initialized)
    }
}