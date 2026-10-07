10 PRINT "Multi-variable NEXT demo"
20 PRINT "---"
30 FOR I = 1 TO 3
40 FOR J = 1 TO 3
50 PRINT I; J;
60 NEXT J, I
70 PRINT
80 PRINT "---"
90 PRINT "Mid-line NEXT with other statements:"
100 FOR I = 1 TO 2
110 FOR J = 1 TO 2
120 PRINT "I="; I; " J="; J; : NEXT J, I
130 PRINT "Done."
140 END
