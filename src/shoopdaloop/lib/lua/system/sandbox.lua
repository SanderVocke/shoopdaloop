-- Based on: http://lua-users.org/wiki/SandBoxes (first simple example)

-- make environment
local env = {
    ipairs = ipairs,
    next = next,
    pairs = pairs,
    pcall = pcall,
    tonumber = tonumber,
    tostring = tostring,
    type = type,
    unpack = unpack,
    coroutine = { create = coroutine.create, resume = coroutine.resume, 
        running = coroutine.running, status = coroutine.status, 
        wrap = coroutine.wrap },
    string = { byte = string.byte, char = string.char, find = string.find, 
        format = string.format, gmatch = string.gmatch, gsub = string.gsub, 
        len = string.len, lower = string.lower, match = string.match, 
        rep = string.rep, reverse = string.reverse, sub = string.sub, 
        upper = string.upper },
    table = { insert = table.insert, maxn = table.maxn, remove = table.remove, 
        sort = table.sort },
    math = { abs = math.abs, acos = math.acos, asin = math.asin, 
        atan = math.atan, atan2 = math.atan2, ceil = math.ceil, cos = math.cos, 
        cosh = math.cosh, deg = math.deg, exp = math.exp, floor = math.floor, 
        fmod = math.fmod, frexp = math.frexp, huge = math.huge, 
        ldexp = math.ldexp, log = math.log, log10 = math.log10, max = math.max, 
        min = math.min, modf = math.modf, pi = math.pi, pow = math.pow, 
        rad = math.rad, random = math.random, sin = math.sin, sinh = math.sinh, 
        sqrt = math.sqrt, tan = math.tan, tanh = math.tanh },
    os = { clock = os.clock, difftime = os.difftime, time = os.time },
    setmetatable = setmetatable,
    error = error,
    print = __shoop_print,
    print_trace = __shoop_print_trace,
    print_debug = __shoop_print_debug,
    print_info = __shoop_print_info,
    print_warning = __shoop_print_warning,
    print_error = __shoop_print_error,
    rawset = rawset,
    rawget = rawget,
    package = package
}

print_trace = __shoop_print_trace
print_debug = __shoop_print_debug
print_info = __shoop_print_info
print_warning = __shoop_print_warning
print_error = __shoop_print_error
print = __shoop_print

-- run code under environment
function __shoop_run_sandboxed(untrusted_code)
  local untrusted_function, message = load(untrusted_code, nil, 't', env)
  if not untrusted_function then 
    error(message)
  end
  return untrusted_function()
end
