      * C14: DISPLAY of packed and binary items.
       IDENTIFICATION DIVISION.
       PROGRAM-ID. SHOWNUM.
       DATA DIVISION.
       WORKING-STORAGE SECTION.
       01  P1 PIC S9(5)V99 COMP-3 VALUE -123.45.
       01  P2 PIC S9(5)V99 COMP-3 VALUE 123.45.
       01  P3 PIC 9(5)V99 COMP-3 VALUE 123.45.
       01  P4 PIC S9(4) COMP-3 VALUE -7.
       01  B1 PIC S9(4) COMP VALUE -12.
       01  B2 PIC S9(4) COMP VALUE 12.
       01  B3 PIC 9(4) COMP VALUE 12.
       01  B4 PIC S9(9) COMP VALUE -123456.
       01  B5 PIC 9(9) COMP VALUE 123456.
       01  B6 PIC S9(18) COMP VALUE -5.
       01  B7 PIC 9(18) COMP VALUE 5.
       01  B8 PIC S9(3)V99 COMP VALUE -1.25.
       01  B9 PIC 99 COMP VALUE 7.
       01  C5 PIC S9(4) COMP-5 VALUE -3.
       01  C6 PIC 9(3) COMP-5 VALUE 3.
       PROCEDURE DIVISION.
           DISPLAY 'P1[' P1 '] P2[' P2 '] P3[' P3 '] P4[' P4 ']'
           DISPLAY 'B1[' B1 '] B2[' B2 '] B3[' B3 '] B4[' B4 ']'
           DISPLAY 'B5[' B5 '] B6[' B6 '] B7[' B7 '] B8[' B8 ']'
           DISPLAY 'B9[' B9 '] C5[' C5 '] C6[' C6 ']'
           GOBACK.
