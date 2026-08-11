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
