10 REM === RANDOM NUMBER GENERATOR TEST ===
20 REM
21 REM This program tests the quality of the RND() random number
22 REM generator by producing 100,000 random integers in the range
23 REM 1-10 and running several statistical checks on the results.
24 REM
25 REM TESTS PERFORMED:
26 REM
27 REM 1. DISTRIBUTION - Counts how many times each value 1-10
28 REM    appears and displays a histogram with percentages. A good
29 REM    RNG should produce roughly equal counts (~10% each).
30 REM
31 REM 2. CHI-SQUARED GOODNESS OF FIT - A standard statistical test
32 REM    for uniformity. For each bin, computes (observed - expected)^2
33 REM    / expected and sums the results. With 10 bins there are 9
34 REM    degrees of freedom; the critical value at p=0.05 is 16.919.
35 REM    If the chi-squared value is below this threshold the
36 REM    distribution is considered uniform (PASS).
37 REM
38 REM 3. COVERAGE - Verifies that every value from 1 to 10 appeared
39 REM    at least once in the sample. With 100,000 trials this should
40 REM    always pass for a working RNG.
41 REM
42 REM 4. RANGE CHECK - Confirms that no generated value fell outside
43 REM    the expected range of 1-10 by tracking the minimum and
44 REM    maximum values seen during generation.
45 REM
46 REM 5. CONSECUTIVE DUPLICATE TEST - Generates a separate sequence
47 REM    of 200 values and counts how many times a value equals the
48 REM    one immediately before it. For a uniform distribution over
49 REM    10 values the expected duplicate rate is 1/10 (about 20 out
50 REM    of 199 pairs). Zero duplicates or an excessively high count
51 REM    (>= 60) would indicate a problem with the RNG.
52 REM
60 PRINT "RANDOM NUMBER GENERATOR TEST"
70 PRINT "============================"
80 PRINT
100 REM --- Count occurrences of each value 1-10 ---
110 DIM C(10)
120 N = 100000
130 MINVAL = 999999
140 MAXVAL = -999999
150 TOTAL = 0
200 REM --- Generate N random numbers ---
210 FOR I = 1 TO N
220   R = INT(RND(1) * 10) + 1
230   C(R) = C(R) + 1
240   IF R < MINVAL THEN MINVAL = R
250   IF R > MAXVAL THEN MAXVAL = R
260   TOTAL = TOTAL + R
270 NEXT I
300 REM --- Display distribution ---
310 PRINT "DISTRIBUTION OF"; N; "RANDOM INTEGERS (1-10):"
320 PRINT
330 EXPECTED = N / 10
340 FOR I = 1 TO 10
350   PCT = INT(C(I) / N * 100 + 0.5)
360   PRINT "  "; I; ":"; C(I); " ("; PCT; "%)";
370   REM --- Draw a simple bar chart ---
380   BARS = INT(C(I) / N * 50 + 0.5)
390   PRINT " ";
400   FOR J = 1 TO BARS
410     PRINT "#";
420   NEXT J
430   PRINT
440 NEXT I
500 REM --- Statistics ---
510 PRINT
520 PRINT "STATISTICS:"
530 MEAN = TOTAL / N
540 PRINT "  SAMPLE SIZE:"; N
550 PRINT "  MIN VALUE:"; MINVAL
560 PRINT "  MAX VALUE:"; MAXVAL
570 PRINT "  MEAN:"; MEAN; " (EXPECTED: 5.5)"
580 PRINT "  EXPECTED PER BIN:"; EXPECTED
600 REM --- Chi-squared goodness of fit ---
610 CHI2 = 0
620 FOR I = 1 TO 10
630   DIFF = C(I) - EXPECTED
640   CHI2 = CHI2 + DIFF * DIFF / EXPECTED
650 NEXT I
660 PRINT "  CHI-SQUARED:"; CHI2
670 REM Critical value for 9 degrees of freedom at p=0.05 is 16.919
680 IF CHI2 < 16.919 THEN PRINT "  RESULT: PASS (P > 0.05)" ELSE PRINT "  RESULT: FAIL (P < 0.05)"
700 REM --- Check all values appeared ---
710 PRINT
720 ALLSEEN = 1
730 FOR I = 1 TO 10
740   IF C(I) = 0 THEN ALLSEEN = 0
750 NEXT I
760 IF ALLSEEN = 1 THEN PRINT "ALL VALUES 1-10 APPEARED: PASS" ELSE PRINT "ALL VALUES 1-10 APPEARED: FAIL"
800 REM --- Check range ---
810 IF MINVAL >= 1 THEN IF MAXVAL <= 10 THEN PRINT "RANGE CHECK (1-10): PASS" ELSE PRINT "RANGE CHECK: FAIL"
820 IF MINVAL < 1 THEN PRINT "RANGE CHECK: FAIL (MIN < 1)"
830 IF MAXVAL > 10 THEN PRINT "RANGE CHECK: FAIL (MAX > 10)"
900 REM --- Consecutive duplicate test ---
910 PRINT
920 DUPS = 0
930 PREV = INT(RND(1) * 10) + 1
940 FOR I = 2 TO 200
950   CURR = INT(RND(1) * 10) + 1
960   IF CURR = PREV THEN DUPS = DUPS + 1
970   PREV = CURR
980 NEXT I
990 PRINT "CONSECUTIVE DUPLICATES IN 200 SAMPLES:"; DUPS
995 REM Expected ~10% for range 1-10
996 IF DUPS > 0 THEN IF DUPS < 60 THEN PRINT "CONSECUTIVE DUPLICATE TEST: PASS" ELSE PRINT "CONSECUTIVE DUPLICATE TEST: FAIL"
997 IF DUPS = 0 THEN PRINT "CONSECUTIVE DUPLICATE TEST: FAIL (NONE)"
1000 PRINT
1010 PRINT "DONE."
1020 END
