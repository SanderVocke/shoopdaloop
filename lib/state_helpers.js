.import '../build/StatesAndActions.js' as StatesAndActions

function is_playing_mode(mode) {
    return [StatesAndActions.LoopMode.Playing, StatesAndActions.LoopMode.PlayingLiveFX].includes(mode)
}