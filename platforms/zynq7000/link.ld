ENTRY(_vector_table)
MEMORY {
  platform (rwx) : ORIGIN = 0x00100000, LENGTH = 0x07f00000
  rust (rw) : ORIGIN = 0x08000000, LENGTH = 0x30000000
}
SECTIONS {
  .text : { KEEP(*(.vectors)) KEEP(*(.boot)) *(.text*) *(.rodata*) *(.glue*) *(.vfp11_veneer) } > platform
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
  .dma (NOLOAD) : { . = ALIGN(0x100000); _end = .; . += 0x200000; __platform_end = .; } > platform
  .rust_heap (NOLOAD) : { __rust_heap = .; . += 0x30000000; __rust_heap_end = .; } > rust
  ASSERT(__platform_end <= 0x08000000, "platform exceeds 128 MiB")
  ASSERT(__rust_heap_end <= 0x38000000, "Rust exceeds 768 MiB arena")
}
