10 REM Fibonacci series generator
20 INPUT "How many Fibonacci numbers? "; N
30 IF N < 1 THEN PRINT "Please enter a positive number." : GOTO 20
40 LET A = 0
50 LET B = 1
60 FOR I = 1 TO N
70   PRINT A; " ";
80   LET C = A + B
90   LET A = B
100  LET B = C
110 NEXT I
120 PRINT
130 END
