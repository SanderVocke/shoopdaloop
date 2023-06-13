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
      const areObjects = isObject(val1) && isObject(val2);
      if(areObjects && !testDeepEqual(val1, val2, log_cb, crumbs.concat([key]))) { return false; }
      else if(!areObjects && val1 !== val2) {
        log_cb(`At ${crumbs.concat([key])}: value unequal (${JSON.stringify(val1, null, 2)} vs ${JSON.stringify(val2, null, 2)})\n`)
        return false;
      }
    }
    return true;
  }

  function isObject(object) {
    return object != null && typeof object === 'object';
  }