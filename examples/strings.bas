10 REM =============================================
20 REM  STRING FUNCTIONS DEMO
30 REM =============================================
40 PRINT
50 PRINT "=== LEN ==="
60 PRINT "LEN(""HELLO"")  ="; LEN("HELLO")
70 PRINT "LEN("""")       ="; LEN("")
80 PRINT "LEN(""A B C"")  ="; LEN("A B C")
90 PRINT
100 PRINT "=== LEFT$, RIGHT$, MID$ ==="
110 W$ = "ABCDEFGHIJ"
120 PRINT "W$ = """; W$; """"
130 PRINT "LEFT$(W$, 3)    = "; LEFT$(W$, 3)
140 PRINT "RIGHT$(W$, 4)   = "; RIGHT$(W$, 4)
150 PRINT "MID$(W$, 4, 3)  = "; MID$(W$, 4, 3)
160 PRINT "MID$(W$, 7)     = "; MID$(W$, 7)
170 PRINT "LEFT$(W$, 0)    = ["; LEFT$(W$, 0); "]"
180 PRINT "RIGHT$(W$, 0)   = ["; RIGHT$(W$, 0); "]"
190 PRINT "LEFT$(W$, 100)  = "; LEFT$(W$, 100)
200 PRINT
210 PRINT "=== INSTR ==="
220 S$ = "HELLO WORLD HELLO"
230 PRINT "S$ = """; S$; """"
240 PRINT "INSTR(S$, ""WORLD"") ="; INSTR(S$, "WORLD")
250 PRINT "INSTR(S$, ""HELLO"") ="; INSTR(S$, "HELLO")
260 PRINT "INSTR(S$, ""GOODBYE"") ="; INSTR(S$, "GOODBYE")
270 PRINT "INSTR(3, S$, ""HELLO"") ="; INSTR(3, S$, "HELLO")
280 PRINT
290 PRINT "=== ASC and CHR$ ==="
300 PRINT "ASC(""A"")  ="; ASC("A")
310 PRINT "ASC(""Z"")  ="; ASC("Z")
320 PRINT "ASC(""0"")  ="; ASC("0")
330 PRINT "CHR$(65)   = "; CHR$(65)
340 PRINT "CHR$(90)   = "; CHR$(90)
350 PRINT "CHR$(48)   = "; CHR$(48)
360 REM Build a string from ASCII codes
370 R$ = ""
380 FOR I = 65 TO 70
390   R$ = R$ + CHR$(I)
400 NEXT I
410 PRINT "A-F built from CHR$: "; R$
420 PRINT
430 PRINT "=== STR$ and VAL ==="
440 PRINT "STR$(42)       = ["; STR$(42); "]"
450 PRINT "STR$(-7)       = ["; STR$(-7); "]"
460 PRINT "STR$(3.14)     = ["; STR$(3.14); "]"
470 PRINT "VAL(""123"")     ="; VAL("123")
480 PRINT "VAL(""-45.6"")   ="; VAL("-45.6")
490 PRINT "VAL(""HELLO"")   ="; VAL("HELLO")
500 PRINT "VAL(STR$(99))  ="; VAL(STR$(99))
510 PRINT
520 PRINT "=== HEX$ and OCT$ ==="
530 PRINT "HEX$(0)    = "; HEX$(0)
540 PRINT "HEX$(255)  = "; HEX$(255)
550 PRINT "HEX$(1000) = "; HEX$(1000)
560 PRINT "OCT$(0)    = "; OCT$(0)
570 PRINT "OCT$(255)  = "; OCT$(255)
580 PRINT "OCT$(1000) = "; OCT$(1000)
590 PRINT
600 PRINT "=== STRING$ and SPACE$ ==="
610 PRINT "STRING$(5, ""*"")  = ["; STRING$(5, "*"); "]"
620 PRINT "STRING$(3, 35)   = ["; STRING$(3, 35); "]"
630 PRINT "SPACE$(8)        = ["; SPACE$(8); "]"
640 PRINT
650 PRINT "=== SPC and TAB (in PRINT) ==="
660 PRINT "A"; SPC(5); "B"; SPC(10); "C"
670 PRINT "X"; TAB(8); "Y"; TAB(8); "Z"
680 PRINT
690 PRINT "=== STRING CONCATENATION ==="
700 A$ = "GOOD"
710 B$ = "BYE"
720 C$ = A$ + " " + B$
730 PRINT "A$ + "" "" + B$ = "; C$
740 PRINT "LEN ="; LEN(C$)
750 PRINT
760 PRINT "=== STRING COMPARISON ==="
770 PRINT """ABC"" = ""ABC"" :"; "ABC" = "ABC"
780 PRINT """ABC"" = ""DEF"" :"; "ABC" = "DEF"
790 PRINT """ABC"" < ""DEF"" :"; "ABC" < "DEF"
800 PRINT """DEF"" > ""ABC"" :"; "DEF" > "ABC"
810 PRINT """ABC"" <> ""DEF"":"; "ABC" <> "DEF"
820 PRINT
830 PRINT "=== MKI$/CVI ROUNDTRIP ==="
840 N = 12345
850 E$ = MKI$(N)
860 PRINT "MKI$("; N; ") -> CVI = "; CVI(E$)
870 PRINT
880 PRINT "=== MKS$/CVS ROUNDTRIP ==="
890 N = 3.14
900 E$ = MKS$(N)
910 PRINT "MKS$("; N; ") -> CVS = "; CVS(E$)
920 PRINT
930 PRINT "=== MKD$/CVD ROUNDTRIP ==="
940 N = 2.718281828
950 E$ = MKD$(N)
960 PRINT "MKD$("; N; ") -> CVD = "; CVD(E$)
970 PRINT
980 REM Build a simple string reversal using MID$ and LEN
990 PRINT "=== STRING REVERSAL ==="
1000 ORIG$ = "STRESSED"
1010 REV$ = ""
1020 FOR I = LEN(ORIG$) TO 1 STEP -1
1030   REV$ = REV$ + MID$(ORIG$, I, 1)
1040 NEXT I
1050 PRINT "REVERSE("""; ORIG$; """) = "; REV$
1060 PRINT
1070 REM Word extraction using INSTR and MID$
1080 PRINT "=== WORD EXTRACTION ==="
1090 PHRASE$ = "THE QUICK BROWN FOX"
1100 PRINT "PHRASE$ = """; PHRASE$; """"
1110 P = 1
1120 WCOUNT = 0
1130 FOR I = 1 TO LEN(PHRASE$)
1140   IF MID$(PHRASE$, I, 1) = " " THEN PRINT "  WORD:"; MID$(PHRASE$, P, I - P) : P = I + 1 : WCOUNT = WCOUNT + 1
1150 NEXT I
1160 PRINT "  WORD:"; MID$(PHRASE$, P)
1170 WCOUNT = WCOUNT + 1
1180 PRINT "  TOTAL WORDS:"; WCOUNT
1190 PRINT
1200 PRINT "=== REPEAT PATTERN WITH STRING$ ==="
1210 FOR I = 1 TO 5
1220   PRINT STRING$(I, "*"); SPACE$(5 - I); STRING$(I, "*")
1230 NEXT I
1240 PRINT
1250 PRINT "DONE."
1260 END
