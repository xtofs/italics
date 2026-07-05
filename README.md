structs for objects

```C
struct Obj_42 {
    int f1;
    void* f2;
};
```

C function

```C
int foo(int x, int y) {
    return x + y;
}
```

```llvm
%Obj_42 = type { i32, ptr }

define i32 @foo(i32 %x, i32 %y) {
    %sum = add i32 %x, %y
    ret i32 %sum
}
```
