#include "IMidiStorageCursor.h"
#include "MidiStorage.h"
#include <stdexcept>

IMidiStorageCursor* CursorFreeFunctions::create_cursor(IMidiStorageCore* storage) {
    // This would need to be implemented with actual cursor creation logic
    // For now, throw since this requires access to the concrete implementation
    throw std::runtime_error("CursorFreeFunctions::create_cursor not yet implemented for interface");
}

IMidiStorageCursor::FindResult CursorFreeFunctions::cursor_find_time_forward(
    IMidiStorageCursor* cursor,
    uint32_t time,
    std::function<void(MidiStorageElem*)> maybe_skip_msg_callback) {
    if (!cursor) {
        return {0, false};
    }
    return cursor->find_time_forward(time, maybe_skip_msg_callback);
}

IMidiStorageCursor::FindResult CursorFreeFunctions::cursor_find_fn_forward(
    IMidiStorageCursor* cursor,
    std::function<bool(MidiStorageElem*)> fn,
    std::function<void(MidiStorageElem*)> maybe_skip_msg_callback) {
    if (!cursor) {
        return {0, false};
    }
    return cursor->find_fn_forward(fn, maybe_skip_msg_callback);
}