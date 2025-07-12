# Specification

```code
VERSION(1B) | CMD(1B) | LENGTH(2B) | STREAMID(4B) | DATA(LENGTH)

VALUES FOR LATEST VERSION:
VERSION:
    1/2

CMD:
    cmdSYN(0)
    cmdFIN(1)
    cmdPSH(2)
    cmdNOP(3)
    cmdUPD(4)	// only supported on version 2

STREAMID:
    client use odd numbers starts from 1
    server use even numbers starts from 0

cmdUPD:
    | CONSUMED(4B) | WINDOW(4B) |
```
