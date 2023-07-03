--  This metatable ensures that we cannot accidentally set/read
--  global variables.
--  See: https://www.lua.org/pil/14.2.html
local __shoop_declared_globals = {}

function global_exists (name)
  if (not __shoop_declared_globals[name]) then
    return false
  else
    return true
  end
end

--  This is the provided way to declare globals.
--  If it already exists, a declaration is ignored.
function declare_global (name, initval)
  if (not global_exists(name)) then
    rawset(_ENV, name, initval)
    __shoop_declared_globals[name] = true
  end
end

--  Alternative method for defining globals which will
--  fail if the global already exists.
function declare_new_global (name, initval)
  if not global_exists(name) then
    declare_global(name, initval)
  else
    error("attempt to declare new global which already exists. "..name, 2)
  end
end

--  Similar to the globals, also provide contexts in which variables
--  can be persisted. This allows for some separation of pseudo-global state
--  between scripts using the same runtime.
local __shoop_current_context = nil
local __shoop_highest_contextid = nil
local __shoop_contexts = {}

function __shoop_new_context()
  if (not __shoop_highest_contextid) then
    __shoop_highest_contextid = 1
  else
    __shoop_highest_contextid = __shoop_highest_contextid + 1
  end
  __shoop_contexts[__shoop_highest_contextid] = {}
  return __shoop_highest_contextid
end

function __shoop_use_context(id)
  if (id and not __shoop_contexts[id]) then
    error("internal error: attempt to use context which does not exist. "..id, 2)
  end
  __shoop_current_context = id
end

function __shoop_set_context (name, v)
  if (not have_context()) then
    error("internal error: attempt to set context var while no context loaded.")
  end
  __shoop_contexts[__shoop_current_context][name] = v
end

function __shoop_get_context (name)
  if (not have_context()) then
    error("internal error: attempt to get context var while no context loaded.")
  end
  return __shoop_contexts[__shoop_current_context][name]
end

function exists_in_context (name)
  if (not __shoop_current_context) then
    return false
  elseif (not __shoop_contexts[__shoop_current_context][name]) then
    return false
  else
    return true
  end
end

function have_context ()
  if (not __shoop_current_context) then
    return false
  else
    return true
  end
end

--  This is the provided way to declare variables in the current
--  context. After declaring, context variables can be accessed
--  directly by name.
function declare_in_context (name, initval)
  if (not have_context()) then
    error("attempt to declare context var while no context loaded.")
  elseif (not exists_in_context (name)) then
    __shoop_set_context (name, initval)
  end
end

function declare_new_in_context (name, initval)
  if (exists_in_context (name)) then
    error("attempt to declare new context var which already exists in context. "..name, 2)
  else
    declare_in_context (name, initval)
  end
end

setmetatable(_ENV, {
  __newindex = function (t, n, v)
    if exists_in_context(n) then
      __shoop_set_context (n, v)
    elseif not global_exists(n) then
      error("attempt to write to undeclared var. "..n, 2)
    else
      rawset(t, n, v)   -- do the actual set
    end
  end,
  __index = function (_, n)
    if (__shoop_current_context and __shoop_contexts[__shoop_current_context][n]) then
      return __shoop_get_context(n)
    elseif not __shoop_declared_globals[n] then
      error("attempt to read undeclared var. "..n, 2)
    else
      return nil
    end
  end,
})

print("LUA runtime ready.")