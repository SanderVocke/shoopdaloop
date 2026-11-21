function Image(image)
  local f = assert(io.open(image.src, "rb"))
  local bytes = f:read("*a")
  f:close()
  image.src = "data:image/svg+xml;base64," .. pandoc.pipe("base64", {}, bytes):gsub("%s", "")
  return image
end
