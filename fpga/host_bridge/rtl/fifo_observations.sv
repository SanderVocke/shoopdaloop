module fifo_observations #(
    parameter integer LOOPS=128,
    parameter integer INDEX_BITS=LOOPS<=1 ? 1 : $clog2(LOOPS)
) (
    input wire clk, reset,
    input wire [31:0] epoch, input wire [63:0] revision,
    input wire gain_valid, input wire [15:0] gain_slot, input wire [31:0] gain_bits,
    input wire event_valid, input wire [255:0] event_data,
    output wire faulted, fault_latched,
    input wire cfg_valid, output wire cfg_ready, input wire cfg_write,
    input wire [15:0] cfg_address, input wire [128:0] cfg_write_data,
    output reg cfg_response_valid, cfg_response_error,
    output reg [128:0] cfg_read_data,
    input wire read_valid, output wire read_ready, input wire [15:0] read_slot,
    output reg response_valid, response_error, output reg [128:0] read_data,
    output reg trace_valid, input wire trace_ready, output reg [255:0] trace_data,
    output reg [63:0] trace_lost,
    output reg [31:0] trace_epoch, output reg [63:0] trace_revision
);
    initial if (LOOPS<1 || LOOPS>128) $fatal(1,"observations require at most 128 loops");
    (* ram_style="distributed" *) reg [31:0] gains [0:LOOPS-1];
    (* ram_style="distributed" *) reg [31:0] failures [0:LOOPS-1];
    (* ram_style="distributed" *) reg [63:0] pulses [0:LOOPS-1];
    (* ram_style="distributed" *) reg pulse_valid [0:LOOPS-1];
    reg retained_fault;
    wire pulse=event_data[7:0]==2;
    wire failure=event_data[7:0]==3 && event_data[23:16]==3;
    wire required_event=event_valid && (pulse || failure);
    wire [31:0] event_slot=event_data[63:32];
    wire [INDEX_BITS-1:0] event_index=event_slot[INDEX_BITS-1:0];
    wire bad_event=required_event && event_slot>=32'(LOOPS);
    wire bad_gain=gain_valid && (gain_slot>=16'(LOOPS) || gain_bits[30:23]==8'hff);
    assign faulted=!reset && (retained_fault || bad_event || bad_gain);
    assign fault_latched=!reset && retained_fault;
    wire updating=gain_valid || required_event;
    assign read_ready=!reset && !faulted && !updating;
    assign cfg_ready=read_ready && !read_valid;
    (* use_dsp="no" *) wire [8:0] cfg_slot=cfg_address[8:0]/9'd3;
    wire [1:0] cfg_word=2'(cfg_address[8:0]-cfg_slot-(cfg_slot<<1));
    wire cfg_in_range=cfg_address<16'(LOOPS*3);
    wire [INDEX_BITS-1:0] cfg_index=cfg_slot[INDEX_BITS-1:0];
    wire cfg_value_valid=cfg_write_data[128:64]==0 && (cfg_word==0
        ? cfg_write_data[63:32]==0 && cfg_write_data[30:23]!=8'hff : cfg_write_data[63:0]==0);
    wire cfg_accepted=cfg_valid && cfg_ready;
    wire initialize=cfg_accepted && cfg_write && cfg_in_range && cfg_value_valid;
    wire [INDEX_BITS-1:0] metadata_index=read_valid ? read_slot[INDEX_BITS-1:0] : cfg_index;
    wire [INDEX_BITS-1:0] gain_address=gain_valid ? gain_slot[INDEX_BITS-1:0] : metadata_index;
    wire [INDEX_BITS-1:0] event_address=required_event ? event_index : metadata_index;
    wire [31:0] gain_value=gains[gain_address];
    wire [31:0] failure_value=failures[event_address];
    wire [63:0] pulse_value=pulses[event_address];
    wire pulse_valid_value=pulse_valid[event_address];
    wire gain_write=gain_valid || (initialize && cfg_word==0);
    wire failure_write=(required_event && failure && !(&failure_value)) || (initialize && cfg_word==0);
    wire pulse_write=(required_event && pulse) || (initialize && cfg_word==1);
    wire pulse_valid_write=(required_event && pulse) || (initialize && cfg_word==2);
    always @(posedge clk) begin
        if (!reset && !faulted) begin
            if (gain_write) gains[gain_address]<=gain_valid ? gain_bits : cfg_write_data[31:0];
            if (failure_write) failures[event_address]<=initialize ? 32'd0 : failure_value+1'b1;
            if (pulse_write) pulses[event_address]<=initialize ? 64'd0 : event_data[255:192];
            if (pulse_valid_write) pulse_valid[event_address]<=!initialize;
        end
        if (reset) begin
            retained_fault<=0; cfg_response_valid<=0; cfg_response_error<=0; cfg_read_data<=0;
            response_valid<=0; response_error<=0; read_data<=0;
            trace_valid<=0; trace_data<=0; trace_lost<=0; trace_epoch<=0; trace_revision<=0;
        end else begin
            if (bad_event || bad_gain) retained_fault<=1;
            response_valid<=read_valid && read_ready;
            response_error<=read_valid && read_ready && read_slot>=16'(LOOPS);
            if (read_valid && read_ready) begin
                if (read_slot<16'(LOOPS))
                    read_data<={pulse_valid_value,pulse_value,failure_value,gain_value};
                else read_data<=0;
            end
            cfg_response_valid<=cfg_accepted;
            cfg_response_error<=cfg_accepted && (!cfg_in_range || (cfg_write && !cfg_value_valid));
            if (cfg_accepted) begin
                cfg_read_data<=0;
                if (cfg_in_range) case (cfg_word)
                    0: cfg_read_data<={65'b0,failure_value,gain_value};
                    1: cfg_read_data<={65'b0,pulse_value};
                    2: cfg_read_data<={128'b0,pulse_valid_value};
                    default: ;
                endcase
            end
            if (trace_ready) trace_valid<=0;
            if (event_valid) begin
                if (!trace_valid || trace_ready) begin
                    trace_valid<=1; trace_data<=event_data; trace_epoch<=epoch; trace_revision<=revision;
                end
                else if (!(&trace_lost)) trace_lost<=trace_lost+1'b1;
            end
        end
    end
endmodule
