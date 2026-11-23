set_param general.maxThreads 2
open_checkpoint /control/post_opt.dcp
set f [open /analysis/register-groups.csv w]
puts $f "group,flip_flops"
foreach group {path_address path_page parent_address cache_address root_data root_valid leaf_valid required_nodes existing_nodes} {
    set count [llength [get_cells -hier -filter "REF_NAME =~ FD* && NAME =~ *${group}_reg*"]]
    puts $f "$group,$count"
}
close $f
exit 0
