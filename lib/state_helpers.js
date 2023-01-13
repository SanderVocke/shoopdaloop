.import '../build/StatesAndActions.js' as StatesAndActions

function is_playing_state(state) {
    return [StatesAndActions.LoopState.Playing, StatesAndActions.LoopState.PlayingMuted, StatesAndActions.LoopState.PlayingLiveFX].includes(state)
}