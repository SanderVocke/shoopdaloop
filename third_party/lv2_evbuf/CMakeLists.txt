add_library(lv2_evbuf SHARED lv2_evbuf.c)
# target_link_libraries(lv2_evbuf PRIVATE
#                      ${LV2_LIBRARIES})
target_include_directories(lv2_evbuf PRIVATE ${LV2_INCLUDE_DIRS})