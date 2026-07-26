c###magvar.for
      SUBROUTINE MAGVAR (CENLAT, CENLG, II, CLG)
C--------------------------------
C
C     ROUTINE TO EVALUATE RAWER MAGNETIC DIP ANGLE AND GYROFREQUENCY
C
C CYCEN IS SUN ZENITH ANGLE, RADIANS.
C II IS SAMPLE AREA
C
C
C CENLAT IS GEOGRAPHIC LATITUDE, RADIANS, OF SAMPLE AREA.
C CENLG IS GEOGRAPHIC LONGITUDE. - IS WEST, + IS EAST
C II IS SAMPLE AREA.,1 TO 5.
C CLG IS GEOGRAPHIC  EAST LONGITUDE.
C
      COMMON /CON /D2R, DCL, GAMA, PI, PI2, PIO2, R2D, RZ, VOFL
      COMMON /GEOG /GYZ (5), RAT (5), GMDIP (5), CLCK (5), ABIY (5), ART
     1IC (5), SIGPAT (5), EPSPAT (5)
      DIMENSION POS (3), UNE (3)
      C360 = 360. * D2R
      IF (CENLG)100, 105, 105
C.....CHANGE TO EAST LONGITUDE
  100 CLG = C360 + CENLG
      GO TO 110
  105 CLG = CENLG
C.....POS(1) IS LATITUDE, POS(2) IS LONGITUDE AND POS(3) IS HEIGHT 300KM
  110 POS (1) = CENLAT
      POS (2) = CLG
      POS (3) = 300000.
C.....EVALUATE THE MAGNETIC FIELD VECTORS
      CALL MAGFIN (POS, UNE)
      HHM = SQRT (UNE (1) * UNE (1) + UNE (2) * UNE (2) + UNE (3) * UNE
     1(3))
C.....GYZ(II) IS THE GYROFREQUENCY
      GYZ (II) = 2.8 * HHM
      TMP = UNE (2) * UNE (2) + UNE (3) * UNE (3)
      GOB = COS (CENLAT)
      IF (GOB .LE. .000001) GOB = .000001
      X = ATAN (ATAN ( - UNE (1) / SQRT (TMP)) / SQRT (GOB))
      GMDIP (II) = X
C.....GMDIP(II) IS THE RAWER MAGNETIC DIP ANGLE
      call trace_magvar(CENLAT,CLG,GYZ(II),GMDIP(II),UNE)
      RETURN
      END
C--------------------------------
      SUBROUTINE trace_magvar(CENLAT,CLG,GYZ1,GMDIP1,UNE)
C     propcore instrumentation: dumps the magnetic field values when the
C     environment variable PROPCORE_TRACE names a directory.
      DIMENSION UNE(3)
      character tdir*256
      call get_environment_variable('PROPCORE_TRACE',tdir,ltdir,ist)
      if(ist.ne.0 .or. ltdir.eq.0) return
      open(96,file=tdir(1:ltdir)//'/magvar.txt',position='append')
      write(96,'(A,7E16.8)') 'MAG ',CENLAT,CLG,GYZ1,GMDIP1,
     A UNE(1),UNE(2),UNE(3)
      close(96)
      RETURN
      END
C--------------------------------
