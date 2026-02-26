3 REM This program finds all prime numbers up to a specified limit.
4 PRINT "Find primes from 2 to what number?";
5 INPUT K
6 IF K < 2 THEN PRINT "Please enter a number greater than or equal to 2." : GOTO 4

10 REM Loop through numbers from 2 to K and check if they are prime.
20 FOR N = 2 TO K
30   LET ISPRIME = 1
40   FOR D = 2 TO SQR(N)
50     IF N / D = INT(N / D) THEN ISPRIME = 0 : GOTO 70
60   NEXT D
70   IF ISPRIME = 1 THEN PRINT N; " ";
80 NEXT N
90 PRINT : PRINT "DONE."
100 END
