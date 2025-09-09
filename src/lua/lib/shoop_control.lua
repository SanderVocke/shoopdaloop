--  Using this library assumes that __shoop_control is already set by the
--  engine's environment.
if not __shoop_control then
    print_error('__shoop_control is not set')
end

return __shoop_control