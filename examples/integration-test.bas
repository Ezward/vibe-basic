10 REM Integration test: exercises the full statement pipeline in one program.
20 REM A STOP is placed last so that, in non-debug mode, it halts execution and
30 REM prints "Break in line 60" while preserving all program state.
40 LET X = 41
50 X = X + 1
60 STOP
