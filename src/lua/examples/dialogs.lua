-- # Script dialog example
--
-- Demonstrates script-owned ShoopDaLoop dialogs using the `shoop_dialog` module.
--
-- ## Windows
--
-- | Window | Kind | Demonstrates |
-- | --- | --- | --- |
-- | Lua dialog example | Simple | Rich text, a callback button, Markdown, and callback links. |
-- | Lua dialog guide | Paged | Multiple pages and retained page selection. |
--
-- The **Apply and show guide** button enables solo mode and opens the paged guide.
-- The Markdown link opens the same guide without changing solo mode.

shoop_announce_api_version(1, 2)

local shoop_control = require('shoop_control')
local shoop_dialog = require('shoop_dialog')

shoop_dialog.simple('Lua dialog example', {
    shoop_dialog.rich_text('This dialog is owned by a Lua script.', { strong = true }),
    shoop_dialog.button('Apply and show guide', function()
        shoop_control.set_solo(true)
        shoop_dialog.open('Lua dialog guide')
    end),
    shoop_dialog.markdown([[
Markdown supports **formatting**, lists, and callback-backed links:

- [Open the guide without changing solo mode](open-guide)
- [Visit the ShoopDaLoop website](https://shoopdaloop.com)
]], {
        ['open-guide'] = function()
            shoop_dialog.open('Lua dialog guide')
        end,
    }),
})

shoop_dialog.paged('Lua dialog guide', {
    {
        shoop_dialog.markdown('Scripts can mix ordered **Markdown**, rich text, and buttons.'),
    },
    {
        shoop_dialog.rich_text('Paged dialogs retain the selected page while they remain active.', { italics = true }),
    },
})

shoop_dialog.open('Lua dialog example')
