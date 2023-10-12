local shoop_format = {}

function shoop_format.format_table(table, recursive)
    local function format_table_recursively(table, recursive, indent)
        local result = ""
        for k, v in pairs(table) do
            result = result .. string.rep(" ", indent) .. k .. ": "
            if type(v) == "table" and recursive then
                result = result .. "\n" .. format_table_recursively(v, recursive, indent + 2)
            else
                result = result .. tostring(v) .. "\n"
            end
        end
        return result
    end

    return format_table_recursively(table, recursive, 0)
end

return shoop_format