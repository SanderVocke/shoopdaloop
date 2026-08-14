-- # Script dialog example
--
-- Demonstrates script-owned ShoopDaLoop dialogs using the `shoop_dialog` module.
--
-- ## Windows
--
-- | Window | Kind | Demonstrates |
-- | --- | --- | --- |
-- | Lua dialog example | Simple | Rich text, a callback button, and opening another dialog. |
-- | Lua dialog guide | Paged | Multiple pages and retained page selection. |
--
-- The **Apply and show guide** button enables solo mode and opens the paged guide.

shoop_announce_api_version(1, 0)

local shoop_control = require('shoop_control')
local shoop_dialog = require('shoop_dialog')

shoop_dialog.simple('Lua dialog example', {
    shoop_dialog.rich_text('This dialog is owned by a Lua script.', { strong = true }),
    shoop_dialog.button('Apply and show guide', function()
        shoop_control.set_solo(true)
        shoop_dialog.open('Lua dialog guide')
    end),
})

shoop_dialog.paged('Lua dialog guide', {
    {
        shoop_dialog.rich_text('Scripts can provide ordered rich text and buttons.'),
    },
    {
        shoop_dialog.rich_text('Paged dialogs retain the selected page while they remain active.', { italics = true }),
    },
})

shoop_dialog.open('Lua dialog example')
