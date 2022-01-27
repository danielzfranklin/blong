FILE *pBinFile = NULL;
unsigned long u4Download_Status = 0;
unsigned long u4ExpectSeq = 0;
unsigned long u4TotalPkt = 0;

void Locus_Dump_Sample_Code(NMEA_MLOX_T *prMLOX)
{
  int i;

  if (prMLOX->u2Type == 0) // LOCUS start
  {
     u4Download_Status = 1; // RECEIVING START

     if (pBinFile)
     {
       fclose(pBinFile);
       pBinFile = NULL;
     }

     pBinFile = fopen("Locus.bin", "w+b");

     u4ExpectSeq = 0;
     u4TotalPkt = prMLOX->u4Data[0];
  }
  else if (prMLOX->u2Type == 1) // LOCUS data
  {
    unsigned long u4Temp = 0;

    if (u4Download_Status == 1)
    {
      u4Download_Status = 2; // RECEIVING DATA
    }

    
    if (u4ExpectSeq != prMLOX->u4Data[0])
    {
      u4Download_Status = 0;
    }

    if (prMLOX->u4Data[0] >= u4TotalPkt)
    {
      u4Download_Status = 0;
    }


    for (i = 1; i < prMLOX->u2FieldNum; i++)
    {
      u4Temp = ((prMLOX->u4Data[i] & 0xFF000000) >> 24) +
               ((prMLOX->u4Data[i] & 0x00FF0000) >> 8) +
               ((prMLOX->u4Data[i] & 0x0000FF00) << 8) +
               ((prMLOX->u4Data[i] & 0x000000FF) << 24);

      if (fwrite((void *)&u4Temp, 1, 4, pBinFile) != 4)
      {
        u4Download_Status = 0;
      }
    }


    u4ExpectSeq = prMLOX->u4Data[0] + 1;
  }
  else if (prMLOX->u2Type == 2) // LOCUS end
  {
    if (u4Download_Status == 2)
    {
      u4Download_Status = 3; // RECEIVING END
    }
    else
    {
      u4Download_Status = 0;
    }


    if (pBinFile)
    {
      fclose(pBinFile);
      pBinFile = NULL;
    }

    Locus_Parser_Start("Locus.bin");


    if (u4Download_Status == 3)
    {
      // print success
    }
    else
    {
      // print fail
    }
  }

}
