# Shoop Lua API version and dialog contract

This contract applies to Lua scripts run by `shoopdaloop` on native and browser targets.

## API version announcement

The current Shoop Lua API version is **1.3**. Every script must make this its first Shoop API call:

```lua
shoop_announce_api_version(1, 3)
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
dialog.markdown(text, link_callbacks)
dialog.markdown_file(relative_path, link_callbacks)
dialog.button(label, callback)
```

`style` is optional. It is a table containing only optional boolean fields:

- `strong`
- `italics`
- `monospace`
- `underline`
- `strikethrough`

`callback` is optional. A button without one is visible but has no action. Unknown style fields, empty button labels, and values of the wrong type are errors.

`link_callbacks` is an optional table mapping Markdown link destinations to Lua functions:

```lua
dialog.markdown([[
Use **Markdown** here. [Enable solo](enable-solo), or
[visit the website](https://shoopdaloop.com).
]], {
    ['enable-solo'] = function()
        require('shoop_control').set_solo(true)
    end,
})
```

A destination present in `link_callbacks` invokes its function instead of opening a URL. Matching is against the complete destination and is local to that Markdown element. Destinations without callbacks retain normal Markdown-link behavior. Link destinations must be non-empty strings and callback values must be functions.

`markdown_file` reads UTF-8 Markdown from the script's resource provider. Filesystem scripts are rooted below the directory containing the Lua file; bundled session/browser scripts use their immutable per-script resource map. It otherwise behaves exactly like `markdown`, including support for callback links:

```lua
dialog.markdown_file('help/getting-started.md', {
    ['enable-solo'] = function()
        require('shoop_control').set_solo(true)
    end,
})
```

## `shoop_file`

Scripts can load any file below their own directory with the `shoop_file` module:

```lua
local file = require('shoop_file')
local contents = file.load('data/preset.bin')
```

`load` returns the file contents as a Lua string, including non-UTF-8 data. The path must be a normalized relative path below the script root. Absolute paths, empty/`.`/`..` components, backslashes, traversal, and filesystem symlinks which resolve outside the root are rejected. Session and hosted-browser bundles use the same checks, reject undeclared or cross-script resources, and require no extraction to disk.

Relative Markdown images resolve from the directory of the Markdown file that contains them. The presentation layer uses generation-scoped `shoop-script-resource://` URIs, so replacing a script cannot reuse stale cached image bytes. Session conversion captures Markdown and the renderer-supported PNG image format; bundled resources are read-only and their archive/storage location is never exposed to Lua.

### Simple dialogs

```lua
dialog.simple(name, {
    dialog.rich_text('Recording is armed.', { strong = true }),
    dialog.markdown('See the **recording controls** below.'),
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
