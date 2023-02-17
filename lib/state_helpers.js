.import '../build/StatesAndActions.js' as StatesAndActions

function is_playing_state(mode) {
    return [StatesAndActions.LoopMode.Playing, StatesAndActions.LoopMode.PlayingLiveFX].includes(mode)
}