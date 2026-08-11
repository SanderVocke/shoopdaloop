# egui Lua API version and dialog contract

This contract applies to Lua scripts run by `shoopdaloop_egui` on native and browser targets.

## API version announcement

The current egui Lua API version is **1.0**. Every script must make this its first Shoop API call:

```lua
shoop_announce_api_version(1, 0)
```

The announcement function is a permanent, unversioned global. Its name and two-integer `(major, minor)` signature do not depend on any module and are reserved to remain stable across future API versions.

A host at `Hmajor.Hminor` accepts a script designed for `Smajor.Sminor` exactly when:

- `Smajor == Hmajor`; and
- `Sminor <= Hminor`.

The host rejects a different major or a script minor newer than its own. It also rejects a missing, repeated, negative, non-integer, or otherwise malformed announcement. Rejection cancels initial script execution at the announcement call, before versioned Shoop APIs can register callbacks, timers, MIDI rules, dialogs, or application mutations. The script's lifecycle and error text report the incompatibility without affecting other scripts.

Existing user and session scripts must add the announcement. The host verifies that the call occurred before allowing versioned Shoop API use.

## `shoop_dialog`

After a compatible announcement, load the dialog module with:

```lua
local dialog = require('shoop_dialog')
```

Dialog names must be non-empty and are unique within one script runtime. Different scripts may use the same visible name.

### Elements

```lua
dialog.rich_text(text, style)
dialog.button(label, callback)
```

`style` is optional. It is a table containing only optional boolean fields:

- `strong`
- `italics`
- `monospace`
- `underline`
- `strikethrough`

`callback` is optional. A button without one is visible but has no action. Unknown style fields, empty button labels, and values of the wrong type are errors.

### Simple dialogs

```lua
dialog.simple(name, {
    dialog.rich_text('Recording is armed.', { strong = true }),
    dialog.button('Stop', function()
        -- ordinary Shoop API calls are allowed here
    end),
})
```

The element sequence must be non-empty and is rendered vertically in order.

### Paged dialogs

```lua
dialog.paged(name, {
    {
        dialog.rich_text('Page one'),
    },
    {
        dialog.rich_text('Page two', { italics = true }),
        dialog.button('Apply', function() end),
    },
})
```

The page sequence and every page must be non-empty. Each page is a simple vertical content sequence. The renderer shows one page at a time with a page control at the bottom.

### Opening

```lua
dialog.open(name)
```

The call requests that an existing named dialog open. It is valid during initial execution and from keyboard, timer, MIDI, application-event, or dialog-button callbacks. Opening an unknown name is an error. Repeated calls can reopen a dialog after the user closes it.

Dialogs are owned by the runtime generation that created them. Stopping, disabling, restarting, replacing, forgetting, finishing, or failing that script removes its dialogs and callbacks. Window visibility, geometry, and current page are presentation state for that runtime generation and are not stored in settings or sessions.
