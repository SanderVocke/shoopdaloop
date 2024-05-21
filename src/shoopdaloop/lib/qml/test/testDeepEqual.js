.import "../../type_checks.js" as TypeChecks

function testDeepEqual(object1, object2, log_cb=console.log, crumbs=[]) {
    const keys1 = Object.keys(object1);
    const keys2 = Object.keys(object2);
    if (keys1.length !== keys2.length) {
      log_cb(`At ${crumbs}: # of keys unequal (${JSON.stringify(object1, null, 2)} vs ${JSON.stringify(object2, null, 2)})\n`)
      return false;
    }
    for (const key of keys1) {
      const val1 = object1[key];
      const val2 = object2[key];
      const areObjects = TypeChecks.is_object(val1) && TypeChecks.is_object(val2);
      if(areObjects && !testDeepEqual(val1, val2, log_cb, crumbs.concat([key]))) { return false; }
      else if(!areObjects && val1 !== val2) {
        log_cb(`At ${crumbs.concat([key])}: value unequal (${JSON.stringify(val1, null, 2)} vs ${JSON.stringify(val2, null, 2)})\n`)
        return false;
      }
    }
    return true;
}

function testArraysCompare(a, b, compare=(a, b) => a == b, log_cb=console.log, crumbs=[]) {
  if (!TypeChecks.is_array(a) || !TypeChecks.is_array(b)) { throw new Error("testArraysCompare only takes arrays") }
  if (a.length != b.length) {
    log_cb(`At ${crumbs}: # of keys unequal (${JSON.stringify(a, null, 2)} vs ${JSON.stringify(b, null, 2)} - lengths ${a.length} vs ${b.length})\n`)
    return false;
  }
  var result = true;
  a.forEach((elem, idx) => {
    const other = b[idx];
    const areArrays = TypeChecks.is_array(elem) && TypeChecks.is_array(other);
    if(areArrays && !testArraysCompare(elem, other, compare, log_cb, crumbs.concat([idx]))) { result = false; }
    else if(!areArrays && !compare(elem, other)) {
      log_cb(`At ${crumbs.concat([idx])}: value compare failed (${elem} vs ${other})\n`)
      result = false;
    }
  })
  return result;
}