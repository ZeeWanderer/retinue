typedef int (*operation)(int);

char marker[] = "wfinspect native graph fixture";

__attribute__((noinline)) int leaf(int value) {
    return value + 0x12345678;
}

__attribute__((noinline)) int wrapper(int value) {
    return leaf(value);
}

__attribute__((naked, noinline)) int tail_jump(int value) {
    __asm__(".byte 0xe9\n\t.long leaf - . - 4");
}

__attribute__((noinline)) int invoke(operation function, int value) {
    return function(value);
}

__attribute__((naked, noinline)) int invoke_jump(operation function, int value) {
    __asm__("mov %rdi, %rax\n\tmov %esi, %edi\n\tjmp *%rax");
}

__attribute__((noinline)) int recursive(int value) {
    return value > 0 ? recursive(value - 1) : leaf(value);
}

operation table[] = {leaf, wrapper};

__attribute__((noinline)) char *marker_address(void) {
    char *pointer;
    __asm__ volatile("lea marker(%%rip), %0" : "=r"(pointer));
    return pointer;
}

int main(int argc, char **argv) {
    return wrapper(argc) + invoke(table[argc % 2], *marker_address())
        + invoke_jump(table[argc % 2], argc) + recursive(argc) + tail_jump(argc);
}
