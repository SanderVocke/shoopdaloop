module audio_stages (
    input wire clk, reset,
    input wire configuration_valid, output wire configuration_ready,
    input wire [7:0] configuration_opcode, input wire [639:0] configuration_data,
    output reg configuration_done, output reg [7:0] configuration_error,
    output wire [63:0] graph_epoch, topology_epoch, output wire loaded,
    input wire request_valid, output wire request_ready,
    input wire [7:0] request_opcode, input wire [511:0] request_data,
    output reg response_valid, output reg [511:0] response_data, output reg fault,
    output wire activation_valid, input wire activation_ready, output wire activation_initial,
    output wire [63:0] activation_nonce, output wire [191:0] activation_spec,
    input wire activation_done, input wire [7:0] activation_error, input wire activation_fault
);
    localparam [1:0] IDLE=0,READY=1,TAKEN=2;
    typedef enum reg [1:0] {WAITING,CHECK,SEND,RESULT} phase_t;
    phase_t phase;
    reg [95:0] graph_id,pair_id;
    reg [63:0] topology_id,validated_epoch,pair_target;
    reg [191:0] topology_spec;
    reg [1:0] graph_status,pair_status,topology_status;
    reg pair_initial,topology_supported,is_loaded;
    reg [63:0] current_graph,current_topology;
    reg [7:0] opcode;
    reg [511:0] payload;
    assign graph_epoch=current_graph;
    assign topology_epoch=current_topology;
    assign loaded=is_loaded;
    assign configuration_ready=phase == WAITING && !request_valid && !fault && !reset;
    assign request_ready=phase == WAITING && !fault && !reset;
    assign activation_valid=phase == SEND;
    assign activation_initial=opcode == 17;
    assign activation_nonce=opcode == 17 ? payload[447:384] : payload[287:224];
    assign activation_spec=topology_spec;
    wire graph_identity=payload[95:0] == graph_id && graph_status != IDLE;
    wire [63:0] expected_graph=opcode == 17 ? payload[319:256] : payload[159:96];
    wire [63:0] expected_topology=opcode == 17 ? payload[383:320] : payload[223:160];
    wire new_graph=configuration_data[31:0] != 0 && configuration_data[63:32] == 0
        && configuration_data[95:64] != 0 && (graph_id == 0 ||
            (configuration_data[31:0] == graph_id[31:0] && !(&graph_id[95:64]) && configuration_data[95:64] == graph_id[95:64]+1'b1));
    wire new_pair=configuration_data[31:0] != 0 && configuration_data[63:32] == 0
        && configuration_data[95:64] != 0 && (pair_id == 0 ||
            (configuration_data[31:0] == pair_id[31:0] && !(&pair_id[95:64]) && configuration_data[95:64] == pair_id[95:64]+1'b1));
    wire new_topology=configuration_data[31:0] != 0 && configuration_data[63:32] != 0
        && (topology_id == 0 || (configuration_data[31:0] == topology_id[31:0]
            && !(&topology_id[63:32]) && configuration_data[63:32] == topology_id[63:32]+1'b1));
    wire unused=&{1'b0,configuration_data[639:257],payload[511:448]};
    task automatic refuse(input [7:0] error);
        begin response_valid<=1;response_data<={8'd1,496'b0,error};phase<=WAITING;end
    endtask
    task automatic configure(input [7:0] error);
        begin configuration_done<=1;configuration_error<=error;end
    endtask
    always @(posedge clk) begin
        configuration_done<=0;response_valid<=0;
        if (reset) begin
            phase<=WAITING;graph_id<=0;pair_id<=0;topology_id<=0;validated_epoch<=0;pair_target<=0;topology_spec<=0;
            graph_status<=IDLE;pair_status<=IDLE;topology_status<=IDLE;pair_initial<=0;topology_supported<=0;
            is_loaded<=0;current_graph<=0;current_topology<=0;opcode<=0;payload<=0;
            configuration_done<=0;configuration_error<=0;response_valid<=0;response_data<=0;fault<=0;
        end else if (!fault) case (phase)
            WAITING: if (request_valid) begin opcode<=request_opcode;payload<=request_data;phase<=CHECK;end
                else if (configuration_valid && configuration_ready) case (configuration_opcode)
                    0: if (!new_graph) configure(1);
                        else if (graph_status != IDLE) configure(4);
                        else if (configuration_data[159:96] != current_graph) configure(10);
                        else begin graph_id<=configuration_data[95:0];validated_epoch<=current_graph;graph_status<=READY;configure(0);end
                    1: if (configuration_data[95:0] != graph_id || graph_status == IDLE) configure(1);
                        else begin graph_status<=IDLE;configure(0);end
                    2: if (!new_pair) configure(1);
                        else if (pair_status != IDLE) configure(4);
                        else begin pair_id<=configuration_data[95:0];pair_target<=configuration_data[159:96];
                            pair_initial<=configuration_data[160];pair_status<=READY;configure(0);end
                    3,4: if (configuration_data[95:0] != pair_id || pair_id == 0) configure(1);
                        else if (configuration_opcode == 4 ? pair_status != TAKEN : pair_status != READY) configure(6);
                        else begin pair_status<=IDLE;pair_initial<=0;configure(0);end
                    5: if (!new_topology) configure(1);
                        else if (topology_status != IDLE) configure(4);
                        else begin topology_id<=configuration_data[63:0];topology_spec<=configuration_data[255:64];
                            topology_supported<=configuration_data[256];topology_status<=READY;configure(0);end
                    6,7: if (configuration_data[63:0] != topology_id || topology_id == 0) configure(1);
                        else if (configuration_opcode == 7 ? topology_status != TAKEN : topology_status != READY) configure(6);
                        else begin topology_status<=IDLE;configure(0);end
                    default: configure(2);
                endcase
            CHECK: if (opcode != 17 && opcode != 18) refuse(2);
                else if (opcode == 18 && expected_topology != current_topology) refuse(10);
                else if (expected_graph != current_graph) refuse(10);
                else if (!graph_identity) refuse(1);
                else if (graph_status != READY) refuse(6);
                else if (validated_epoch != expected_graph) refuse(10);
                else if (&current_graph) refuse(5);
                else if (opcode == 17) begin
                    if (payload[191:96] != pair_id || pair_id == 0) refuse(1);
                    else if (pair_status != READY) refuse(6);
                    else if (payload[255:192] != topology_id || topology_id == 0) refuse(1);
                    else if (topology_status != READY) refuse(6);
                    else if (expected_topology != current_topology) refuse(10);
                    else if (is_loaded) refuse(4);
                    else if (!topology_supported) refuse(2);
                    else if (!pair_initial) refuse(6);
                    else if (pair_target != {topology_spec[32:1],32'b0}) refuse(1);
                    else if (&current_topology) refuse(5);
                    else phase<=SEND;
                end else phase<=SEND;
            SEND: if (activation_ready) phase<=RESULT;
            RESULT: if (activation_done) begin
                if (activation_fault) begin fault<=1;refuse(6);end
                else if (activation_error != 0) refuse(activation_error);
                else begin
                    graph_status<=TAKEN;current_graph<=current_graph+1'b1;response_data<=0;response_valid<=1;phase<=WAITING;
                    response_data[95:0]<=graph_id;
                    if (opcode == 17) begin
                        pair_status<=TAKEN;topology_status<=TAKEN;is_loaded<=1;current_topology<=current_topology+1'b1;
                        response_data[511:504]<=7;response_data[191:96]<=pair_id;response_data[255:192]<=topology_id;
                        response_data[319:256]<=current_graph+1'b1;response_data[383:320]<=current_topology+1'b1;
                    end else begin
                        response_data[511:504]<=6;response_data[159:96]<=current_graph+1'b1;response_data[223:160]<=current_topology;
                    end
                end
            end
            default: begin fault<=1;refuse(6);end
        endcase
    end
endmodule
