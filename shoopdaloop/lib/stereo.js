function balance(l_volume, r_volume) {
    var max = Math.max(l_volume, r_volume)
    var min = Math.min(l_volume, r_volume)
    var offset_ratio = 1.0 - min/max
    var offset_sign = (l_volume < r_volume) ? -1.0 : 1.0
    return offset_sign * offset_ratio
}

function individual_volumes(overall_volume, balance) {
    var max = overall_volume
    var min = (1.0-Math.abs(balance)) * overall_volume
    return balance >= 0.0 ? [min, max] : [max, min]
}