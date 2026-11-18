module host_mailbox #(
    parameter logic [31:0] CORE_CLOCK_HZ = 100_000_000,
    parameter logic [31:0] ARENA_BASE = 32'h30000000,
    parameter logic [31:0] ARENA_BYTES = 32'h08000000
) (
    input wire clk,
    input wire reset,
    input wire [11:0] s_axi_awaddr,
    input wire s_axi_awvalid,
    output wire s_axi_awready,
    input wire [31:0] s_axi_wdata,
    input wire [3:0] s_axi_wstrb,
    input wire s_axi_wvalid,
    output wire s_axi_wready,
    output reg [1:0] s_axi_bresp,
    output reg s_axi_bvalid,
    input wire s_axi_bready,
    input wire [11:0] s_axi_araddr,
    input wire s_axi_arvalid,
    output wire s_axi_arready,
    output reg [31:0] s_axi_rdata,
    output reg [1:0] s_axi_rresp,
    output reg s_axi_rvalid,
    input wire s_axi_rready,
    input wire [31:0] epoch,
    input wire fault,
    output wire irq,
    output wire command_valid,
    input wire command_ready,
    output reg [31:0] command_tag,
    output reg [31:0] command_op,
    output reg [32*host_registers::HOST_PAYLOAD_WORDS-1:0] command_data,
    input wire response_valid,
    output wire response_ready,
    input wire [31:0] response_epoch,
    input wire [31:0] response_tag,
    input wire [31:0] response_code,
    input wire [32*host_registers::HOST_PAYLOAD_WORDS-1:0] response_data,
    input wire sync_event_valid,
    output wire sync_event_ready,
    input wire [255:0] sync_event_data,
    output reg event_overflow
);
    import host_registers::*;

    localparam [1:0] OKAY = 2'b00, SLVERR = 2'b10;
    localparam integer WORD_INDEX_BITS = $clog2(HOST_PAYLOAD_WORDS);
    localparam [11:0] REQUEST_END = REG_REQUEST_DATA + 12'(4*HOST_PAYLOAD_WORDS);
    localparam [11:0] RESPONSE_END = REG_RESPONSE_DATA + 12'(4*HOST_PAYLOAD_WORDS);
    localparam [11:0] EVENT_END = REG_EVENT_DATA + 12'd32;
    localparam integer EVENT_DEPTH = 128;
    localparam integer EVENT_INDEX_BITS = $clog2(EVENT_DEPTH);

    reg aw_pending, w_pending;
    reg [11:0] write_address;
    reg [31:0] write_data;
    reg [3:0] write_strobes;

    reg [31:0] live_epoch, staged_epoch, staged_op;
    reg [32*HOST_PAYLOAD_WORDS-1:0] staged_data;
    reg command_pending, response_pending;
    reg [31:0] saved_tag, saved_code;
    reg [32*HOST_PAYLOAD_WORDS-1:0] saved_data;
    reg [255:0] events [0:EVENT_DEPTH-1];
    reg [EVENT_INDEX_BITS-1:0] event_head, event_tail;
    reg [EVENT_INDEX_BITS:0] event_count;
    reg [31:0] event_tag;

    wire session_live = epoch != 0 && epoch == live_epoch && !reset;
    wire request_space = session_live && !fault && !command_pending;
    wire [WORD_INDEX_BITS-1:0] write_index = WORD_INDEX_BITS'((write_address-REG_REQUEST_DATA) >> 2);
    wire [WORD_INDEX_BITS-1:0] read_index = WORD_INDEX_BITS'((s_axi_araddr-REG_RESPONSE_DATA) >> 2);
    wire [2:0] event_index = 3'((s_axi_araddr-REG_EVENT_DATA) >> 2);
    wire event_pending = session_live && event_count != 0;
    wire event_pop = aw_pending && w_pending && write_strobes == 4'hf && write_address == REG_EVENT_POP
        && event_pending && write_data == event_tag;
    wire event_push = sync_event_valid && sync_event_ready && (event_count < (EVENT_INDEX_BITS+1)'(EVENT_DEPTH) || event_pop);

    assign s_axi_awready = !reset && !aw_pending && !s_axi_bvalid;
    assign s_axi_wready = !reset && !w_pending && !s_axi_bvalid;
    assign s_axi_arready = !reset && !s_axi_rvalid;
    assign command_valid = session_live && command_pending && !fault;
    assign response_ready = session_live && !response_pending;
    assign irq = session_live && (response_pending || event_pending || fault);
    assign sync_event_ready = session_live;

    always @(posedge clk) begin
        if (event_push)
            events[event_tail] <= sync_event_data;
        if (!session_live) begin
            event_head <= 0;
            event_tail <= 0;
            event_count <= 0;
            event_tag <= 1;
            event_overflow <= 0;
        end else begin
            if (event_pop) begin
                event_head <= event_head+1'b1;
                event_tag <= event_tag == 32'hffffffff ? 32'd1 : event_tag+1'b1;
            end
            if (event_push)
                event_tail <= event_tail+1'b1;
            if (sync_event_valid && !event_push)
                event_overflow <= 1;
            case ({event_push,event_pop})
                2'b10: event_count <= event_count+1'b1;
                2'b01: event_count <= event_count-1'b1;
                default: ;
            endcase
        end
    end

    always @(posedge clk) begin
        if (reset) begin
            aw_pending <= 0;
            w_pending <= 0;
            s_axi_bresp <= OKAY;
            s_axi_bvalid <= 0;
            s_axi_rvalid <= 0;
            s_axi_rresp <= OKAY;
            s_axi_rdata <= 0;
            live_epoch <= 0;
            write_address <= 0;
            write_data <= 0;
            write_strobes <= 0;
        end else begin
            live_epoch <= epoch;
            if (s_axi_awready && s_axi_awvalid) begin
                write_address <= s_axi_awaddr;
                aw_pending <= 1;
            end
            if (s_axi_wready && s_axi_wvalid) begin
                write_data <= s_axi_wdata;
                write_strobes <= s_axi_wstrb;
                w_pending <= 1;
            end
            if (s_axi_bready)
                s_axi_bvalid <= 0;
            if (aw_pending && w_pending) begin
                aw_pending <= 0;
                w_pending <= 0;
                s_axi_bvalid <= 1;
                s_axi_bresp <= SLVERR;
                if (write_strobes == 4'hf && write_address[1:0] == 0 && session_live) begin
                    if (write_address == REG_REQUEST_EPOCH || write_address == REG_REQUEST_OP ||
                        (write_address >= REG_REQUEST_DATA && write_address < REQUEST_END))
                        s_axi_bresp <= OKAY;
                    if (write_address == REG_REQUEST_PUSH && request_space && staged_epoch == epoch && write_data != 0)
                        s_axi_bresp <= OKAY;
                    if (write_address == REG_RESPONSE_POP && response_pending && write_data == saved_tag)
                        s_axi_bresp <= OKAY;
                    if (event_pop)
                        s_axi_bresp <= OKAY;
                end
            end
            if (s_axi_rready)
                s_axi_rvalid <= 0;
            if (s_axi_arvalid && s_axi_arready) begin
                s_axi_rvalid <= 1;
                s_axi_rresp <= OKAY;
                s_axi_rdata <= 0;
                case (s_axi_araddr)
                    REG_ID: s_axi_rdata <= HOST_ID;
                    REG_VERSION: s_axi_rdata <= HOST_VERSION;
                    REG_STATUS: begin
                        s_axi_rdata[STATUS_REQUEST_SPACE] <= request_space;
                        s_axi_rdata[STATUS_RESPONSE_PENDING] <= session_live && response_pending;
                        s_axi_rdata[STATUS_FAULT] <= fault;
                    end
                    REG_EPOCH: s_axi_rdata <= epoch;
                    REG_CORE_CLOCK_HZ: s_axi_rdata <= CORE_CLOCK_HZ;
                    REG_ARENA_BASE: s_axi_rdata <= ARENA_BASE;
                    REG_ARENA_BYTES: s_axi_rdata <= ARENA_BYTES;
                    REG_RESPONSE_TAG: s_axi_rdata <= session_live && response_pending ? saved_tag : 0;
                    REG_RESPONSE_CODE: s_axi_rdata <= session_live && response_pending ? saved_code : 0;
                    REG_EVENT_COUNT: s_axi_rdata <= session_live ? {event_overflow, {(30-EVENT_INDEX_BITS){1'b0}}, event_count} : 0;
                    REG_EVENT_TAG: s_axi_rdata <= event_pending ? event_tag : 0;
                    default: begin
                        if (s_axi_araddr >= REG_RESPONSE_DATA && s_axi_araddr < RESPONSE_END && s_axi_araddr[1:0] == 0)
                            s_axi_rdata <= session_live && response_pending ? saved_data[32*read_index +: 32] : 0;
                        else if (s_axi_araddr >= REG_EVENT_DATA && s_axi_araddr < EVENT_END && s_axi_araddr[1:0] == 0)
                            s_axi_rdata <= event_pending ? events[event_head][32*event_index +: 32] : 0;
                        else
                            s_axi_rresp <= SLVERR;
                    end
                endcase
            end
        end
    end

    always @(posedge clk) begin
        if (!session_live) begin
            staged_epoch <= 0;
            staged_op <= 0;
            staged_data <= 0;
            command_pending <= 0;
            command_tag <= 0;
            command_op <= 0;
            command_data <= 0;
            response_pending <= 0;
            saved_tag <= 0;
            saved_code <= 0;
            saved_data <= 0;
        end else begin
            if ((command_ready && command_valid) || fault)
                command_pending <= 0;
            if (response_valid && response_ready && response_epoch == epoch) begin
                response_pending <= 1;
                saved_tag <= response_tag;
                saved_code <= response_code;
                saved_data <= response_data;
            end
            if (aw_pending && w_pending && write_strobes == 4'hf && write_address[1:0] == 0) begin
                case (write_address)
                    REG_REQUEST_EPOCH: staged_epoch <= write_data;
                    REG_REQUEST_OP: staged_op <= write_data;
                    REG_REQUEST_PUSH: if (request_space && staged_epoch == epoch && write_data != 0) begin
                        command_pending <= 1;
                        command_tag <= write_data;
                        command_op <= staged_op;
                        command_data <= staged_data;
                    end
                    REG_RESPONSE_POP: if (response_pending && write_data == saved_tag)
                        response_pending <= 0;
                    default: if (write_address >= REG_REQUEST_DATA && write_address < REQUEST_END)
                        staged_data[32*write_index +: 32] <= write_data;
                endcase
            end
        end
    end
endmodule
