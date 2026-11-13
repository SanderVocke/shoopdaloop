ENTRY(_vector_table)
MEMORY {
  platform (rwx) : ORIGIN = 0x00100000, LENGTH = @PLATFORM_LENGTH@
  rust (rw) : ORIGIN = @RUST_START@, LENGTH = @RUST_LENGTH@
}
SECTIONS {
  .text : { KEEP(*(.vectors)) KEEP(*(.boot)) *(.text*) *(.rodata*) KEEP(*(.usb_observation)) *(.glue*) *(.vfp11_veneer) } > platform
  .ARM.extab : { *(.ARM.extab*) } > platform
  .ARM.exidx : { __exidx_start = .; *(.ARM.exidx*) __exidx_end = .; } > platform
  .init_array : {
    __init_array_start = .; KEEP(*(SORT(.init_array.*))) KEEP(*(.init_array))
    __init_array_end = .;
  } > platform
  .fini_array : { __fini_array_start = .; KEEP(*(.fini_array*)) __fini_array_end = .; } > platform
  .data : { *(.data*) } > platform
  .mmu_tbl ALIGN(16384) : { *(.mmu_tbl) } > platform
  .bss (NOLOAD) : {
    . = ALIGN(16); __bss_start = .; __sbss_start = .; *(.sbss*) __sbss_end = .;
    *(.bss*) *(COMMON) . = ALIGN(16); __bss_end = .;
  } > platform
  .heap (NOLOAD) : {
    _heap = .; HeapBase = .; . += 0x01000000; _heap_end = .; HeapLimit = .;
  } > platform
  .stack (NOLOAD) : {
    . = ALIGN(16); _stack_end = .;
    . += 0x10000; __undef_stack = .;
    . += 0x10000; __abort_stack = .;
    . += 0x10000; __fiq_stack = .;
    . += 0x10000; __irq_stack = .;
    . += 0x10000; __supervisor_stack = .;
    . += 0x100000; __stack = .; _stack = .;
  } > platform
  .usb_dma ALIGN(0x100000) (NOLOAD) : {
    __usb_dma_start = .; *(.usb_dma*) . = ALIGN(0x100000); __usb_dma_end = .;
  } > platform
  .dma (NOLOAD) : { . = ALIGN(0x100000); _end = .; . += 0x200000; __platform_end = .; } > platform
  .rust_heap (NOLOAD) : { __rust_heap = .; . += @RUST_LENGTH@; __rust_heap_end = .; } > rust
  ASSERT(__platform_end <= @RUST_START@, "platform exceeds board region")
  ASSERT(__usb_dma_end - __usb_dma_start <= 0x100000, "USB DMA exceeds 1 MiB")
  ASSERT(__rust_heap_end <= @RUST_END@, "Rust exceeds board arena")
}
