function balance(l_gain, r_gain) {
    var max = Math.max(l_gain, r_gain)
    var min = Math.min(l_gain, r_gain)
    var offset_ratio = 1.0 - min/max
    var offset_sign = (l_gain > r_gain) ? -1.0 : 1.0
    return offset_sign * offset_ratio
}

function individual_gains(overall_gain, balance) {
    var max = overall_gain
    var min = (1.0-Math.abs(balance)) * overall_gain
    return balance >= 0.0 ? [min, max] : [max, min]
}