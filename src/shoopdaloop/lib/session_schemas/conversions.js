.import '../generated/types.js' as Types

const channel_modes = {
    'direct': Types.ChannelMode.Direct,
    'disabled': Types.ChannelMode.Disabled,
    'dry': Types.ChannelMode.Dry,
    'wet': Types.ChannelMode.Wet
}

function parse_channel_mode(mode_str) {
    return channel_modes[mode_str]
}

function stringify_channel_mode(mode) {
    var e = Object.entries(channel_modes)
    for(var i=0; i<e.length; i++) {
        if(e[i][1] == mode) { return e[i][0] }
    }
    throw new Error("Invalid mode", mode)
}