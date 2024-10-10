function blocking_delay(ms) {
    let start = (new Date()).getTime()
    while(true) {
        let curr = (new Date()).getTime()
        if ((curr - start) >= ms) {
            break;
        }
    }
}