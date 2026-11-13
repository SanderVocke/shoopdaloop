module media_metadata #(
    parameter integer VIEWS = 32, parameter integer SLOTS = 32, parameter integer CHUNK_FRAMES = 32,
    parameter integer TABLE_PAGES = (SLOTS+14)/15 + 8*VIEWS + 16,
    parameter integer RETIRED_TABLES = 256,
    parameter integer TABLE_LINES = 8, parameter integer REF_LINES = 4, parameter integer STACK_LINES = 2,
    parameter integer BURST_MEMORY = 1,
    parameter integer LEVELS = (SLOTS < 2) ? 1 : ($clog2(SLOTS)+3)/4,
    parameter integer VIEW_BITS = (VIEWS < 2) ? 1 : $clog2(VIEWS)
) (
    input wire clk, input wire reset, input wire [63:0] arena_base,
    input wire request_valid, output wire request_ready, input wire [3:0] request_opcode,
    input wire [31:0] request_view, input wire [31:0] request_index, input wire [31:0] request_data,
    output wire response_valid, input wire response_ready,
    output reg [31:0] response_data, output reg [7:0] response_error, output wire fault,
    output reg [31:0] retired_count, output reg [31:0] retired_high_water,
    output reg [31:0] retired_head, output reg [31:0] retired_tail,
    output reg [63:0] retired_references,
    output wire [31:0] pages_used, output wire [31:0] pages_available, output reg [63:0] pages_freed,
    output wire memory_valid, input wire memory_ready, output wire memory_write,
    output wire [63:0] memory_address, output wire [31:0] memory_write_data, output wire [31:0] memory_count,
    output wire memory_write_valid, input wire memory_write_ready,
    input wire memory_read_valid, output wire memory_read_ready,
    input wire memory_response_valid, output wire memory_response_ready,
    input wire memory_response_error, input wire [31:0] memory_read_data
);
    localparam [63:0] SAMPLE_WORDS = ((64'(SLOTS)*64'(CHUNK_FRAMES)+15)/16)*16;
    localparam [63:0] SLOT_WORDS = ((64'(SLOTS)+15)/16)*16;
    localparam [63:0] REF_BASE = SAMPLE_WORDS*4;
    localparam [63:0] STACK_BASE = (SAMPLE_WORDS+SLOT_WORDS)*4;
    localparam [63:0] PAGE_BASE = (SAMPLE_WORDS+2*SLOT_WORDS)*4;
    localparam [63:0] QUEUE_BASE = PAGE_BASE+64'(TABLE_PAGES)*64;
    typedef enum logic [5:0] {IDLE, ROOT_CAPTURE, ROOT, WALK, CHILD, ACCESS_DONE, PUBLISH, RESPONSE,
        CACHE_SEND, CACHE_WAIT, ALLOCATE, ALLOCATE_LINK, ALLOCATE_ZERO, ALLOCATE_ATTACH,
        TRIM_COUNT, TRIM_CHECK, TRIM_NEXT, FREE_PAGE, FREE_LINK, SCAN_START, SCAN_WORD, SCAN_RESULT, FLUSH_NEXT,
        CHECK_TREE, COUNT_NODES, CAPACITY, CHECK_CAPACITY, DETACH_ROOT, DETACH_COUNT, DETACH_COMMIT,
        GC_ROOT, GC_COUNT, GC_WALK, GC_SAVE_ROOT, GC_SAVE_COUNT, GC_COMMIT} phase_t;
    phase_t phase, resume;
    reg [VIEWS-1:0] root_valid, leaf_valid;
    wire [127:0] root_ram_data;
    reg [127:0] root_data;
    localparam integer LEVEL_BITS = LEVELS < 2 ? 1 : $clog2(LEVELS);
    reg [31:0] view, ordinal, payload, root, used, page;
    reg [LEVEL_BITS-1:0] level;
    reg invalid_tree_request, invalid_read_extent, invalid_write_extent, cached_leaf_match;
    reg [31:0] high_water, free_head, free_pages;
    reg [31:0] allocated, parent_page, cached_leaf, cached_index;
    reg [31:0] new_count;
    reg [31:0] gc_root, gc_used;
    reg gc_loaded;
    reg [63:0] required_nodes, existing_nodes, unallocated_pages, additional, available;
    reg capacity_grow, capacity_insufficient;
    reg [63:0] operations [0:9];
    reg [63:0] directory_reads, table_reads;
    wire unused_operations = &{1'b0,directory_reads,table_reads,operations[0],operations[1],operations[2],operations[3],operations[4],operations[5],operations[6],operations[7],operations[8],operations[9]};
    reg [63:0] parent_address;
    reg [31:0] path_page [0:LEVELS-1];
    reg [63:0] path_address [0:LEVELS-1];
    reg [31:0] trim_count, trim_start;
    reg [31:0] trim_used_pages, trim_requested_pages;
    reg trim_finished;
    reg root_exists, checked;
    reg [3:0] opcode;
    reg [1:0] bank, cache_opcode;
    reg [63:0] cache_address;
    reg [31:0] cache_data, value;
    reg cache_invalidate;
    wire [2:0] cache_ready, cache_valid, cache_error, cache_fault;
    wire [95:0] cache_result;
    wire [2:0] bus_valid, bus_ready, bus_write, bus_write_valid, bus_write_ready;
    wire [2:0] bus_read_valid, bus_read_ready, bus_response_valid, bus_response_ready, bus_response_error;
    wire [191:0] bus_address;
    wire [95:0] bus_write_data, bus_count, bus_read_data;
    assign fault = |cache_fault;
    assign pages_used = high_water-free_pages;
    assign pages_available = 32'(TABLE_PAGES)-high_water+free_pages;
    assign request_ready = phase == IDLE && !reset && !fault;
    assign response_valid = phase == RESPONSE && !reset;
    assign memory_valid = bank < 3 && bus_valid[bank];
    assign memory_write = bus_write[bank];
    assign memory_address = arena_base+bus_address[bank*64+:64];
    assign memory_write_data = bus_write_data[bank*32+:32];
    assign memory_count = bus_count[bank*32+:32];
    assign memory_write_valid = bank < 3 && bus_write_valid[bank];
    assign memory_read_ready = bank < 3 && bus_read_ready[bank];
    assign memory_response_ready = bank < 3 && bus_response_ready[bank];
    function automatic [63:0] page_address(input [31:0] id);
        page_address = PAGE_BASE+(64'(id)-1)*64;
    endfunction
    function automatic [63:0] nodes(input [31:0] count);
        reg [63:0] n, total;
        begin
            n = {32'b0,count}; total = 0;
            for (integer i=0; i<LEVELS; i=i+1) begin n = (n+15) >> 4; total = total+n; end
            nodes = total;
        end
    endfunction
    wire [63:0] entry_address = page_address(page)+(({32'b0,ordinal} >> (32'(level)*4)) & 64'd15)*4;
    wire bad_tree_request = (opcode != 11 && view >= 32'(VIEWS)) || ordinal > 32'(SLOTS)
        || (opcode < 2 && ordinal == 32'(SLOTS));
    media_ram #(.WIDTH(128),.DEPTH(VIEWS),.ADDR_BITS(VIEW_BITS)) roots (
        .clk(clk),.read_enable(request_valid && request_ready && request_view < 32'(VIEWS)),.read_address(VIEW_BITS'(request_view)),.read_data(root_ram_data),
        .write_enable(phase == PUBLISH && opcode != 12),.write_address(VIEW_BITS'(view)),.write_data({cached_leaf,cached_index,used,root})
    );
    generate for (genvar i=0; i<3; i=i+1) begin : banks
        wire [63:0] hits, misses, fills, writebacks;
        wire unused_statistics = &{1'b0,hits,misses,fills,writebacks};
        assign bus_ready[i] = bank == i && memory_ready;
        assign bus_write_ready[i] = bank == i && memory_write_ready;
        assign bus_read_valid[i] = bank == i && memory_read_valid;
        assign bus_read_data[i*32+:32] = memory_read_data;
        assign bus_response_valid[i] = bank == i && memory_response_valid;
        assign bus_response_error[i] = memory_response_error;
        metadata_cache #(.LINES(i == 0 ? TABLE_LINES : i == 1 ? REF_LINES : STACK_LINES),.BURST_MEMORY(BURST_MEMORY)) cache (
            .clk(clk),.reset(reset),.request_valid(phase == CACHE_SEND && bank == i),.request_ready(cache_ready[i]),
            .request_opcode(cache_opcode),.request_invalidate(cache_invalidate),.request_address(cache_address),.request_data(cache_data),
            .response_valid(cache_valid[i]),.response_ready(phase == CACHE_WAIT && bank == i),
            .response_data(cache_result[i*32+:32]),.response_error(cache_error[i]),.fault(cache_fault[i]),
            .hits(hits),.misses(misses),.fills(fills),.writebacks(writebacks),
            .memory_valid(bus_valid[i]),.memory_ready(bus_ready[i]),.memory_write(bus_write[i]),
            .memory_address(bus_address[i*64+:64]),.memory_write_data(bus_write_data[i*32+:32]),.memory_count(bus_count[i*32+:32]),
            .memory_write_valid(bus_write_valid[i]),.memory_write_ready(bus_write_ready[i]),
            .memory_read_valid(bus_read_valid[i]),.memory_read_ready(bus_read_ready[i]),
            .memory_response_valid(bus_response_valid[i]),.memory_response_ready(bus_response_ready[i]),
            .memory_response_error(bus_response_error[i]),.memory_read_data(bus_read_data[i*32+:32])
        );
    end endgenerate
    task automatic access_cache(input [1:0] which, input [1:0] op, input [63:0] address,
                                 input [31:0] data, input phase_t next);
        begin bank <= which; cache_opcode <= op; cache_address <= address; cache_data <= data; resume <= next; phase <= CACHE_SEND; end
    endtask
    initial if (TABLE_PAGES < 1 || LEVELS < 1 || LEVELS > 8 || RETIRED_TABLES < 1) $fatal(1,"Invalid metadata geometry");
    always @(posedge clk) begin
        if (reset) begin
            phase <= IDLE; resume <= IDLE; root_valid <= 0; leaf_valid <= 0; root_data <= 0; view <= 0; ordinal <= 0; payload <= 0;
            root <= 0; used <= 0; page <= 0; level <= 0; high_water <= 0; free_head <= 0; free_pages <= 0;
            invalid_tree_request <= 0; invalid_read_extent <= 0; invalid_write_extent <= 0; cached_leaf_match <= 0;
            allocated <= 0; parent_page <= 0; cached_leaf <= 0; cached_index <= 0; parent_address <= 0; trim_count <= 0; trim_start <= 0;
            trim_used_pages <= 0; trim_requested_pages <= 0; trim_finished <= 0;
            new_count <= 0; required_nodes <= 0; existing_nodes <= 0;
            retired_head <= 0; retired_tail <= 0; retired_count <= 0; retired_high_water <= 0;
            retired_references <= 0; pages_freed <= 0; gc_root <= 0; gc_used <= 0; gc_loaded <= 0;
            unallocated_pages <= 0; additional <= 0; available <= 0; capacity_grow <= 0; capacity_insufficient <= 0;
            root_exists <= 0; checked <= 0; opcode <= 0; bank <= 0;
            cache_opcode <= 0; cache_address <= 0; cache_data <= 0; value <= 0; cache_invalidate <= 0;
            response_data <= 0; response_error <= 0;
            directory_reads <= 0; table_reads <= 0;
            for (integer i=0; i<10; i=i+1) operations[i] <= 0;
        end else begin
            case (phase)
                IDLE: if (request_valid && request_ready) begin
                    if (request_opcode < 10) operations[request_opcode] <= operations[request_opcode]+1;
                    view <= request_view; ordinal <= request_index; payload <= request_data; opcode <= request_opcode;
                    new_count <= request_opcode == 1 ? request_index+1'b1 : request_index;
                    root_exists <= request_view < 32'(VIEWS) && root_valid[VIEW_BITS'(request_view)];
                    checked <= 0; response_error <= 0; response_data <= 0; cache_invalidate <= request_opcode == 9;
                    bank <= 0; phase <= ROOT_CAPTURE;
                end
                ROOT_CAPTURE: begin root_data <= root_ram_data; phase <= ROOT; end
                ROOT: begin
                    root <= root_exists ? root_data[31:0] : 0;
                    used <= root_exists ? root_data[63:32] : 0;
                    cached_leaf <= root_exists && leaf_valid[VIEW_BITS'(view)] ? root_data[127:96] : 0;
                    cached_index <= root_data[95:64];
                    if (opcode == 9) leaf_valid <= 0;
                    if (opcode == 6) begin leaf_valid[VIEW_BITS'(view)] <= 0; cached_leaf <= 0; end
                    if (opcode == 10) begin
                        if (view >= 32'(VIEWS)) begin response_error <= 1; phase <= RESPONSE; end
                        else if (!root_exists || root_data[63:32] == 0) begin
                            root <= 0; used <= 0; cached_leaf <= 0; phase <= PUBLISH;
                        end else if (retired_count == 32'(RETIRED_TABLES)) begin response_error <= 3; phase <= RESPONSE; end
                        else phase <= DETACH_ROOT;
                    end else if (opcode == 11) begin
                        if (retired_count == 0 || gc_loaded) begin response_error <= 2; phase <= RESPONSE; end
                        else access_cache(0,0,QUEUE_BASE+64'(retired_head)*64,0,GC_ROOT);
                    end else if (opcode == 12) begin
                        if (!gc_loaded || gc_used == 0) begin response_error <= 2; phase <= RESPONSE; end
                        else begin
                            root <= gc_root; used <= gc_used; cached_leaf <= 0;
                            trim_count <= gc_used-1; phase <= TRIM_COUNT;
                        end
                    end else if (opcode == 13) begin
                        if (ordinal >= 32'(RETIRED_TABLES)*16) begin response_error <= 1; phase <= RESPONSE; end
                        else access_cache(0,0,QUEUE_BASE+64'(ordinal)*4,0,ACCESS_DONE);
                    end else if (opcode == 14) begin
                        case (ordinal)
                            0: response_data <= high_water;
                            1: response_data <= free_pages;
                            2: response_data <= retired_head;
                            3: response_data <= retired_tail;
                            default: response_error <= 1;
                        endcase
                        phase <= RESPONSE;
                    end else if (opcode == 8 || opcode == 9) phase <= FLUSH_NEXT;
                    else if (opcode == 15) begin response_error <= 2; phase <= RESPONSE; end
                    else if (opcode >= 2 && opcode <= 5) begin
                        if (ordinal >= 32'(SLOTS)) begin response_error <= 1; phase <= RESPONSE; end
                        else access_cache(opcode < 4 ? 2'd1 : 2'd2, {1'b0,opcode[0]},
                            (opcode < 4 ? REF_BASE : STACK_BASE)+64'(ordinal)*4,payload,ACCESS_DONE);
                    end else begin
                        level <= LEVEL_BITS'(LEVELS-1); parent_page <= 0;
                        page <= root_exists ? root_data[31:0] : 0;
                        phase <= CHECK_TREE;
                    end
                end
                CHECK_TREE, GC_WALK: begin
                    invalid_tree_request <= bad_tree_request;
                    invalid_read_extent <= opcode == 0 && ordinal >= used;
                    invalid_write_extent <= (opcode == 1 || opcode == 6) && ordinal > used;
                    cached_leaf_match <= cached_leaf != 0 && cached_index == (ordinal >> 4);
                    phase <= opcode == 1 || opcode == 7 ? COUNT_NODES : WALK;
                end
                COUNT_NODES: begin
                    required_nodes <= nodes(new_count);
                    existing_nodes <= nodes(used);
                    capacity_grow <= new_count > used;
                    unallocated_pages <= 64'(TABLE_PAGES)-{32'b0,high_water};
                    phase <= CAPACITY;
                end
                CAPACITY: begin
                    additional <= capacity_grow ? required_nodes-existing_nodes : 64'b0;
                    available <= unallocated_pages+{32'b0,free_pages};
                    phase <= CHECK_CAPACITY;
                end
                CHECK_CAPACITY: begin
                    capacity_insufficient <= additional > available;
                    phase <= WALK;
                end
                WALK: begin
                    checked <= 1;
                    if (invalid_tree_request) begin
                        response_error <= 1; phase <= RESPONSE;
                    end else if (!checked && (opcode == 1 || opcode == 7) && capacity_insufficient) begin
                        response_error <= 3; phase <= RESPONSE;
                    end else if (invalid_read_extent) begin response_error <= 1; phase <= RESPONSE; end
                    else if (invalid_write_extent) begin response_error <= 4; phase <= RESPONSE; end
                    else if (opcode == 7) phase <= RESPONSE;
                    else if (opcode == 6) begin trim_count <= ordinal; phase <= TRIM_COUNT; end
                    else if (!checked && cached_leaf_match) begin
                        page <= cached_leaf; level <= 0;
                    end else if (page == 0) begin
                        if (opcode == 0 || opcode == 11) begin response_error <= 1; phase <= RESPONSE; end
                        else phase <= ALLOCATE;
                    end else begin
                        path_page[level] <= page; path_address[level] <= entry_address;
                        if (level == 0) begin
                            table_reads <= table_reads+64'(opcode == 0);
                            access_cache(0,opcode == 1 ? 2'd1 : 2'd0,entry_address,payload,ACCESS_DONE);
                        end else begin
                            directory_reads <= directory_reads+1; access_cache(0,0,entry_address,0,CHILD);
                        end
                    end
                end
                CHILD: begin
                    parent_page <= page; parent_address <= path_address[level];
                    level <= level-1'b1; page <= value;
                    phase <= WALK;
                end
                ALLOCATE: begin
                    if (free_pages != 0) begin
                        allocated <= free_head;
                        access_cache(0,0,page_address(free_head),0,ALLOCATE_LINK);
                    end else if (high_water < 32'(TABLE_PAGES)) begin
                        allocated <= high_water+1; high_water <= high_water+1; phase <= ALLOCATE_ZERO;
                    end else begin response_error <= 3; phase <= RESPONSE; end
                end
                ALLOCATE_LINK: begin free_head <= value; free_pages <= free_pages-1; phase <= ALLOCATE_ZERO; end
                ALLOCATE_ZERO: access_cache(0,2,page_address(allocated),0,ALLOCATE_ATTACH);
                ALLOCATE_ATTACH: begin
                    page <= allocated;
                    if (parent_page == 0) begin root <= allocated; phase <= WALK; end
                    else access_cache(0,1,parent_address,allocated,WALK);
                end
                ACCESS_DONE: begin
                    response_data <= value;
                    if (opcode < 2) begin
                        cached_leaf <= page; cached_index <= ordinal >> 4;
                        if (opcode == 1 && ordinal >= used) used <= ordinal+1;
                        phase <= PUBLISH;
                    end else phase <= RESPONSE;
                end
                PUBLISH: begin
                    if (opcode == 12) phase <= GC_SAVE_ROOT;
                    else begin
                        root_valid[VIEW_BITS'(view)] <= 1;
                        leaf_valid[VIEW_BITS'(view)] <= opcode < 2; phase <= RESPONSE;
                    end
                end
                DETACH_ROOT: access_cache(0,1,QUEUE_BASE+64'(retired_tail)*64,root,DETACH_COUNT);
                DETACH_COUNT: access_cache(0,1,QUEUE_BASE+64'(retired_tail)*64+4,used,DETACH_COMMIT);
                DETACH_COMMIT: begin
                    response_data <= used;
                    retired_references <= retired_references+{32'b0,used};
                    retired_count <= retired_count+1;
                    if (retired_count+1 > retired_high_water) retired_high_water <= retired_count+1;
                    retired_tail <= retired_tail+1 == 32'(RETIRED_TABLES) ? 0 : retired_tail+1;
                    root <= 0; used <= 0; cached_leaf <= 0; phase <= PUBLISH;
                end
                GC_ROOT: begin gc_root <= value; access_cache(0,0,QUEUE_BASE+64'(retired_head)*64+4,0,GC_COUNT); end
                GC_COUNT: begin
                    gc_used <= value; used <= value; root <= gc_root;
                    if (value == 0 || value > 32'(SLOTS) || gc_root == 0) begin response_error <= 2; phase <= RESPONSE; end
                    else begin
                        gc_loaded <= 1; ordinal <= value-1; cached_leaf <= 0;
                        level <= LEVEL_BITS'(LEVELS-1); page <= gc_root; phase <= GC_WALK;
                    end
                end
                GC_SAVE_ROOT: access_cache(0,1,QUEUE_BASE+64'(retired_head)*64,root,GC_SAVE_COUNT);
                GC_SAVE_COUNT: access_cache(0,1,QUEUE_BASE+64'(retired_head)*64+4,used,GC_COMMIT);
                GC_COMMIT: begin
                    gc_loaded <= 0; gc_root <= root; gc_used <= used;
                    retired_references <= retired_references-1;
                    if (used == 0) begin
                        retired_count <= retired_count-1;
                        retired_head <= retired_head+1 == 32'(RETIRED_TABLES) ? 0 : retired_head+1;
                    end
                    response_data <= used; phase <= RESPONSE;
                end
                RESPONSE: if (response_ready) phase <= IDLE;
                CACHE_SEND: if (cache_ready[bank]) phase <= CACHE_WAIT;
                CACHE_WAIT: if (cache_valid[bank]) begin
                    value <= cache_result[bank*32+:32];
                    if (cache_error[bank]) begin response_error <= 5; phase <= RESPONSE; end
                    else phase <= resume;
                end
                TRIM_COUNT: begin
                    trim_used_pages <= (used+15) >> 4; trim_requested_pages <= (trim_count+15) >> 4;
                    phase <= TRIM_CHECK;
                end
                TRIM_CHECK: begin trim_finished <= trim_used_pages <= trim_requested_pages; phase <= TRIM_NEXT; end
                TRIM_NEXT: begin
                    if (trim_finished) begin used <= trim_count; phase <= PUBLISH; end
                    else begin
                        ordinal <= ((used-1) >> 4) << 4; trim_start <= ((used-1) >> 4) << 4;
                        page <= root; level <= LEVEL_BITS'(LEVELS-1); parent_page <= 0;
                        phase <= SCAN_START;
                    end
                end
                SCAN_START: begin
                    path_page[level] <= page; path_address[level] <= entry_address;
                    if (level == 0) phase <= FREE_PAGE;
                    else access_cache(0,0,entry_address,0,SCAN_WORD);
                end
                SCAN_WORD: begin page <= value; level <= level-1'b1; phase <= SCAN_START; end
                FREE_PAGE: access_cache(0,1,page_address(page),free_head,FREE_LINK);
                FREE_LINK: begin
                    free_head <= page; free_pages <= free_pages+1; pages_freed <= pages_freed+1;
                    if (32'(level)+1 == 32'(LEVELS)) begin root <= 0; used <= trim_start; phase <= TRIM_COUNT; end
                    else begin
                        page <= path_page[32'(level)+1]; level <= level+1'b1;
                        if (((ordinal >> ((32'(level)+1)*4)) & 15) == 0) phase <= FREE_PAGE;
                        else access_cache(0,1,path_address[32'(level)+1],0,SCAN_RESULT);
                    end
                end
                SCAN_RESULT: begin used <= trim_start; phase <= TRIM_COUNT; end
                FLUSH_NEXT: begin
                    if (bank == 3) phase <= RESPONSE;
                    else access_cache(bank,3,0,0,SCAN_WORD);
                end
                default: begin response_error <= 2; phase <= RESPONSE; end
            endcase
            // A fence advances banks only after each cache has acknowledged its drain.
            if (phase == SCAN_WORD && (opcode == 8 || opcode == 9)) begin bank <= bank+1; phase <= FLUSH_NEXT; end
        end
    end
endmodule
