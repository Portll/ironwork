      * C101: a ROUNDED receiver's extra decimal place reaches the
      * intermediate results under --dialect ibm alone.
       IDENTIFICATION DIVISION.
       PROGRAM-ID. ROUNDED.
       DATA DIVISION.
       WORKING-STORAGE SECTION.
       01  A PIC 9(5)V99 VALUE 1.
       01  B PIC 9(5)V99 VALUE 3.
       01  C PIC 9(3) VALUE 100.
       01  D PIC 9(5)V99 VALUE 1.
       01  E PIC 9(5)V99 VALUE 12.35.
       01  DIV2 PIC 99V9 VALUE 44.1.
       01  DIV3 PIC 9(4)V9 VALUE 1661.7.
       01  X PIC 9(5)V99.
       01  Y PIC 9(5)V99.
       01  S PIC 99V9.
       01  Z PIC S99V9 COMP-3.
       PROCEDURE DIVISION.
           COMPUTE D ROUNDED = D + E / 3
           COMPUTE Y ROUNDED = A / B * C
           COMPUTE X ROUNDED = (A / B) + (A / B)
           DISPLAY 'INNER ' D ' ' Y ' ' X
           COMPUTE S ROUNDED = 1 + 1661.7 / DIV2
           COMPUTE Z ROUNDED = - (DIV3 / DIV2)
           DISPLAY 'INNER ' S ' ' Z
           COMPUTE S ROUNDED = 1661.7 / DIV2
           DIVIDE DIV2 INTO DIV3 ROUNDED
           DIVIDE 3 INTO 2 GIVING X ROUNDED REMAINDER Y
           DISPLAY 'LAST ' S ' ' DIV3 ' ' X ' ' Y
           GOBACK.
