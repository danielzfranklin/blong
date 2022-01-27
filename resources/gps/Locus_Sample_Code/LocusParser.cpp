//---------------------------------------------------------------------------
#include <vcl.h>
#include <time.h>
#include <stdio.h>
#include <math.h>
#include <stdlib.h>
#include <inifiles.hpp>
#include <dir.h>
#include <stdio.h>
#include "LocusParser.h"
#include <tchar.h>
//---------------------------------------------------------------------------

#define LOCUS_MAX_HEADER2_BIT_NUM  (7)
#define LOCUS_HEADER_SIZE          (64)
#define LOCUS_HEADER1_SIZE         (16)
#define LOCUS_HEADER1_CS_BUF_SIZE  (14)
#define LOCUS_HEADER2_SIZE         (44)
#define LOCUS_DATA_SIZE            (4032)
#define LOCUS_DATA_CHECKSUM_SIZE   (1)
#define SECTOR_SIZE                (4096)
// Locus Collect Data Bit-Map
#define LOCUS_CONTENT_UTC    (1 << 0)   // 4-byte
#define LOCUS_CONTENT_VALID  (1 << 1)   // 1-byte // Modified
#define LOCUS_CONTENT_LAT    (1 << 2)   // 4-byte
#define LOCUS_CONTENT_LON    (1 << 3)   // 4-byte
#define LOCUS_CONTENT_HGT    (1 << 4)   // 2-byte // Modified
#define LOCUS_CONTENT_SPD    (1 << 5)   // 2-byte // Modified
#define LOCUS_CONTENT_TRK    (1 << 6)   // 2-byte // Modified
#define LOCUS_CONTENT_HDOP   (1 << 10)  // 2-byte
#define LOCUS_CONTENT_NSAT   (1 << 12)  // 1-byte // Modified

unsigned char* _DataImage = NULL;
unsigned int   _uDataSize = 0;
unsigned int u4Content = 0;
unsigned short u2Serial = 0;
AnsiString _sInputDATAName;
AnsiString _sOutputLogName;
AnsiString _sOutputNMEAName;

 unsigned short u2Locus_Gen_Checksum(unsigned short *pBuf, unsigned short u2Size)
{
   unsigned short i;
   unsigned short LocCkSum = 0;

   for(i=0; i<u2Size; i++)
   {
       LocCkSum ^= (pBuf[i]);
   }

   return LocCkSum;
}


 bool Locus_Read_Header( unsigned int SectorBaseAddr)
{
    unsigned short CheckSumInFile = 0;
    unsigned short LocCheckSum = 0;
    char CheckSumBuf[LOCUS_HEADER1_CS_BUF_SIZE];
    bool ReadOk = false;

    // Read locus data checksum
    memcpy(&CheckSumInFile, &_DataImage[SectorBaseAddr+LOCUS_HEADER1_CS_BUF_SIZE], sizeof(CheckSumInFile));
    // Compute locus data checksum
    memcpy(&CheckSumBuf[0],&_DataImage[SectorBaseAddr],LOCUS_HEADER1_CS_BUF_SIZE);
    LocCheckSum = u2Locus_Gen_Checksum((unsigned short*)CheckSumBuf,(LOCUS_HEADER1_CS_BUF_SIZE/2));

    // read Checksum
    if( LocCheckSum == CheckSumInFile)
    {
         u2Serial  = *(unsigned short *)&_DataImage[SectorBaseAddr];
         u4Content = *(unsigned int *)&_DataImage[SectorBaseAddr+4];
         ReadOk = true;
    }
    else
    {
         ReadOk = false;
    }

    return ReadOk;
}

 static double Translate_Nmea_Deg_Min_Sec (double deg)
 {
     int  intpart = (int)deg;
     double fracpart = deg - (double)intpart;

     if (fracpart < .0)
     {
         fracpart = -fracpart;
     }
     if (intpart < 0)
     {
         intpart = -intpart;
     }
     fracpart *= 60.0;

     return  ((double)(intpart) * 100.0) + fracpart;
 }

 unsigned char u1Locus_Gen_Checksum(unsigned char *pBuf, unsigned short u2Size)
{
   unsigned short i;
   unsigned char LocCkSum = 0;

   for(i=0; i<u2Size; i++)
   {
      LocCkSum ^= (pBuf[i]);
   }
   return LocCkSum;
}

unsigned Locus_Capture_Data( char* filebuf, char* nmeabuf, unsigned int BaseDataAddr, unsigned int* FileBufLength, unsigned int* NmeaBufLength)
{
    char OutBuf[256];
    char OutNMEABuf[256];
    char strbuf[128];
    unsigned int OutputIdx = 0;
    unsigned int OutNMEAIdx = 0;
    unsigned short NMEALength = 0, tmpHDOP = 0;
    unsigned int DataAddrIdx = BaseDataAddr;
    unsigned char u1TmpNSAT = 0x00;
    unsigned char u1VALID = 0x00;  // 0:NoFix , 1: Fix, 2: DGPS, 6: Estimated
    short tmpHGT = 0;
    short tmpSPD = 0;
    unsigned short tmpTRK= 0;
    float tmpLAT = 0;
    float tmpLON = 0;
    struct tm *pTM;

    memset(OutBuf,0x00,sizeof(OutBuf));

    if ( u4Content & LOCUS_CONTENT_UTC)
    {
        time_t  UTC_time;
        UTC_time = *(unsigned int *)&_DataImage[DataAddrIdx];
        pTM = gmtime ( &UTC_time );

        memset(strbuf,0x00,sizeof(strbuf));
        snprintf( strbuf, sizeof(strbuf),"UTC : %04d/%02d/%02d,%02d:%02d:%02d\r\n",
                          (pTM->tm_year+1900),
                          (pTM->tm_mon+1),
                           pTM->tm_mday,
                           pTM->tm_hour,
                           pTM->tm_min ,
                           pTM->tm_sec );
        snprintf((OutBuf+OutputIdx), sizeof(OutBuf),"%s",strbuf);
        OutputIdx = OutputIdx +  strlen(strbuf);
        DataAddrIdx = DataAddrIdx + 4;
    }

    if(u4Content & LOCUS_CONTENT_VALID)
    {
        unsigned char u1TmpVALID = 0;
        u1TmpVALID = *(unsigned char *)&_DataImage[DataAddrIdx];

        memset(strbuf,0x00,sizeof(strbuf));
        if ((u1TmpVALID & 0x04) == 0x04)
        {
            snprintf( strbuf, sizeof(strbuf),"FixType:3D-fix\r\n");
            u1VALID = 2;
        }
        else if ((u1TmpVALID & 0x02) == 0x02)
        {
            snprintf( strbuf, sizeof(strbuf),"Fix Type: 3D-fix\r\n");
            u1VALID = 1;
        }
        else if ((u1TmpVALID & 0x40) == 0x40)
        {
            snprintf( strbuf, sizeof(strbuf),"Fix Type: Estimated\r\n");
            u1VALID = 6;
        }
        else if (u1TmpVALID == 0x00)
        {
            snprintf( strbuf, sizeof(strbuf),"Fix Type: No-fix\r\n");
            u1VALID = 0;
        }

        snprintf((OutBuf+OutputIdx), sizeof(OutBuf),"%s",strbuf);
        OutputIdx = OutputIdx +  strlen(strbuf);
        DataAddrIdx = DataAddrIdx + 1;
    }

    if ( u4Content & LOCUS_CONTENT_LAT)
    {
        tmpLAT  = *(float *)&_DataImage[DataAddrIdx];
        memset(strbuf,0x00,sizeof(strbuf));
        snprintf( strbuf, sizeof(strbuf),"LAT:%f degree\r\n",tmpLAT);
        snprintf((OutBuf+OutputIdx), sizeof(OutBuf),"%s",strbuf);
        OutputIdx = OutputIdx +  strlen(strbuf);
        DataAddrIdx = DataAddrIdx + 4;
    }

    if ( u4Content & LOCUS_CONTENT_LON)
    {
        tmpLON  = *(float *)&_DataImage[DataAddrIdx];
        memset(strbuf,0x00,sizeof(strbuf));
        snprintf( strbuf, sizeof(strbuf),"LON:%f degree\r\n",tmpLON);
        snprintf((OutBuf+OutputIdx), sizeof(OutBuf),"%s",strbuf);
        OutputIdx = OutputIdx +  strlen(strbuf);
        DataAddrIdx = DataAddrIdx + 4;
    }

    if ( u4Content & LOCUS_CONTENT_HGT)
    {
        tmpHGT = *(short *)&_DataImage[DataAddrIdx];
        memset(strbuf,0x00,sizeof(strbuf));
        snprintf( strbuf, sizeof(strbuf),"HGT:%d meter\r\n",tmpHGT);
        snprintf((OutBuf+OutputIdx), sizeof(OutBuf),"%s",strbuf);
        OutputIdx = OutputIdx +  strlen(strbuf);
        DataAddrIdx = DataAddrIdx + 2;
    }
    if ( u4Content & LOCUS_CONTENT_SPD)
    {
        tmpSPD = *(short *)&_DataImage[DataAddrIdx];
        memset(strbuf,0x00,sizeof(strbuf));
        snprintf( strbuf, sizeof(strbuf),"Speed:%d m/s\r\n",tmpSPD);
        snprintf((OutBuf+OutputIdx), sizeof(OutBuf),"%s",strbuf);
        OutputIdx = OutputIdx +  strlen(strbuf);
        DataAddrIdx = DataAddrIdx + 2;
    }
    if (u4Content & LOCUS_CONTENT_TRK)
    {
        tmpTRK = *(unsigned short *)&_DataImage[DataAddrIdx];
        memset(strbuf,0x00,sizeof(strbuf));
        snprintf( strbuf, sizeof(strbuf),"Track:%d degree\r\n",tmpTRK);
        snprintf((OutBuf+OutputIdx), sizeof(OutBuf),"%s",strbuf);
        OutputIdx = OutputIdx +  strlen(strbuf);
        DataAddrIdx = DataAddrIdx + 2;
    }
    if (u4Content & LOCUS_CONTENT_HDOP)
    {
        tmpHDOP = *(unsigned short *)&_DataImage[DataAddrIdx];
		memset(strbuf,0x00,sizeof(strbuf));
        snprintf( strbuf, sizeof(strbuf),"HDOP:%d\n",tmpHDOP);
        snprintf((OutBuf+OutputIdx), sizeof(OutBuf),"%s",strbuf);
        OutputIdx = OutputIdx +  strlen(strbuf);
        DataAddrIdx = DataAddrIdx + 2;
    }
    if (u4Content & LOCUS_CONTENT_NSAT)
    {
        u1TmpNSAT = *(unsigned char *)&_DataImage[DataAddrIdx];

        memset(strbuf,0x00,sizeof(strbuf));
        snprintf(strbuf, sizeof(strbuf),"InUsedSV:%d\r\n", u1TmpNSAT);
        snprintf((OutBuf+OutputIdx), sizeof(OutBuf),"%s",strbuf);
        OutputIdx = OutputIdx +  strlen(strbuf);
        DataAddrIdx = DataAddrIdx + 1;
    }

    // Log NMEA
    {
        int slen = 0;
        memset(OutNMEABuf,0x00,sizeof(OutNMEABuf));
        /* GPGGA */

        memset(strbuf,0x00,sizeof(strbuf));
        snprintf( strbuf, (sizeof(strbuf)),
                  "GPGGA,%02d%02d%02d.%03d,%09.4f,%c,%010.4f,%c,%d,%d,",
                  pTM->tm_hour, pTM->tm_min,
                  pTM->tm_sec, 0,
                  Translate_Nmea_Deg_Min_Sec(tmpLAT), ((tmpLAT >= .0) ? 'N' : 'S'),
                  Translate_Nmea_Deg_Min_Sec(tmpLON), ((tmpLON >= .0) ? 'E' : 'W'),
                  u1VALID, u1TmpNSAT);
       slen = (int)strlen(strbuf);
       snprintf( &strbuf[slen], (sizeof(strbuf)-slen), ",%.1f,M,,M",tmpHGT);

       slen += (int)strlen(&strbuf[slen]);
       snprintf( &strbuf[slen], (sizeof(strbuf)-slen), ",,");

       NMEALength = snprintf( (OutNMEABuf+OutNMEAIdx), sizeof(OutNMEABuf), "$%s*%02X\r\n", strbuf, u1Locus_Gen_Checksum(strbuf, strlen(strbuf)));
       OutNMEAIdx = OutNMEAIdx +  NMEALength;

       /* GPRMC */

       memset(strbuf,0x00,sizeof(strbuf));
       snprintf( strbuf, sizeof(strbuf),
                 "GPRMC,%02d%02d%02d.%03d,%c,%09.4f,%c,%010.4f,%c,%.3f,%.2f,%02d%02d%02d,,%c",
                 pTM->tm_hour, pTM->tm_min,
                 pTM->tm_sec, 0,
                 ((u1VALID >= 1) ? 'A' : 'V'),
                 Translate_Nmea_Deg_Min_Sec(tmpLAT), ((tmpLAT >= .0) ? 'N' : 'S'),
                 Translate_Nmea_Deg_Min_Sec(tmpLON), ((tmpLON >= .0) ? 'E' : 'W'),
                 (tmpSPD * 1.942795467), (float)tmpTRK,
                 (pTM->tm_mday), (pTM->tm_mon+1),
                 (pTM->tm_year+1900)%100,
                 (u1VALID >= 1 ?
                 (u1VALID == 2 ? 'D' : 'A') :
                 (u1VALID == 6 ? 'E' : 'N')));
       NMEALength = snprintf( (OutNMEABuf+OutNMEAIdx), sizeof(OutNMEABuf), "$%s*%02X\r\n", strbuf, u1Locus_Gen_Checksum(strbuf, strlen(strbuf)));
       OutNMEAIdx = OutNMEAIdx +  NMEALength;
    }

    memcpy(filebuf, OutBuf, OutputIdx);
    memcpy(nmeabuf, OutNMEABuf, OutNMEAIdx);
    (*FileBufLength) = OutputIdx;
    (*NmeaBufLength) = OutNMEAIdx;

    return TRUE;
}

 unsigned char uCalaulateSize()
 {
    unsigned char LocalDataSize =0;
    // Size of data content

    if (u4Content & LOCUS_CONTENT_UTC)
    {
       LocalDataSize = LocalDataSize + 4;
    }
    if (u4Content & LOCUS_CONTENT_VALID)
    {
       LocalDataSize = LocalDataSize + 1;
    }
    if (u4Content & LOCUS_CONTENT_LAT)
    {
       LocalDataSize = LocalDataSize + 4;
    }
    if (u4Content & LOCUS_CONTENT_LON)
    {
       LocalDataSize = LocalDataSize + 4;
    }
    if (u4Content & LOCUS_CONTENT_HGT)
    {
       LocalDataSize = LocalDataSize + 2;
    }
    if (u4Content & LOCUS_CONTENT_TRK)
    {
       LocalDataSize = LocalDataSize + 2;
    }
    if (u4Content & LOCUS_CONTENT_SPD)
    {
       LocalDataSize = LocalDataSize + 2;
    }
    if (u4Content & LOCUS_CONTENT_HDOP)
    {
       LocalDataSize = LocalDataSize + 2;
    }
    if (u4Content & LOCUS_CONTENT_NSAT)
    {
       LocalDataSize = LocalDataSize + 1;
    }

	LocalDataSize = LocalDataSize + 1;
	return LocalDataSize;
 }

unsigned int Locus_Find_BitMap(unsigned int SectorBaseAddr)
{
    short i = 0,j = 0;
    unsigned char NumByte = 0, NumBit= 0;
    unsigned char Data = 0;
    unsigned int total_packet = 0;
         
    for(i=(LOCUS_HEADER2_SIZE-1);i>=0;i--)
    {
        unsigned int Addr;
        Addr = (SectorBaseAddr+LOCUS_HEADER1_SIZE) +i;
       
            // read data
        Data = _DataImage[Addr];
        if ( Data != 0xFF)
        {
            break;
        }
    }
         
    if ( i >= 0) // found byte
    {
        for (j=0;j<=LOCUS_MAX_HEADER2_BIT_NUM;j++)
        {
            if( (Data>>j)==0 )
            {
                break;
            }
        }
            // Get how many data in this sector
         NumByte = i;
         if( j==0 )
         {
             NumByte = NumByte + 1;         	
             NumBit = 0;
         }
         else
         {
             NumBit  = (LOCUS_MAX_HEADER2_BIT_NUM+1)-j;
         }
    }
    else
    {
          NumByte = 0;
          NumBit = 0;
    }
    total_packet =  NumByte*8 + NumBit;
    return total_packet;
}

 bool  Locus_Parser()
 {
    char FileBuffer[256];
    char NMEABuffer[256];
    char strbuf[128];
    unsigned char PacketDataSize = 0;
    unsigned int TotalPacketNum = 0;
    unsigned int DataIndex = 0;
    unsigned int SectorIndex = 0;
    unsigned int PacketIndex = 0;
    unsigned int PacketCount = 0;
    unsigned int PacketAddr = 0;
    unsigned int FileBufLength  =  0;
    unsigned int NMEABufLength  =  0;
    int iHandle = 0, iHandleNMEA = 0;
    unsigned int SectorNum =  ceil(_uDataSize/SECTOR_SIZE);
    bool ret = true;
    
    iHandle = FileCreate(_sOutputLogName);
    iHandleNMEA =  FileCreate(_sOutputNMEAName);
    if (iHandle >= 0 && iHandleNMEA >= 0)
    {
        FileClose(iHandle);
        FileClose(iHandleNMEA);
        iHandle = FileOpen(_sOutputLogName, fmOpenWrite | fmShareDenyWrite);
        iHandleNMEA = FileOpen(_sOutputNMEAName, fmOpenWrite | fmShareDenyWrite);
        if (iHandle >= 0 && iHandleNMEA >= 0)
        {
           for ( SectorIndex = 0; SectorIndex < SectorNum ; SectorIndex++)
           {
               DataIndex = SectorIndex*SECTOR_SIZE;
               // Read Header  , Update Contect , Update Serial

               if (Locus_Read_Header(DataIndex) == true)
               {
                   // Calaulate packet size
                   PacketDataSize = uCalaulateSize();
                   // Find total number of packets in the sector
                   TotalPacketNum = Locus_Find_BitMap(DataIndex);
                   // Update Sector size
                   PacketIndex = 0;
                   PacketAddr  = LOCUS_HEADER_SIZE;

                   while ( PacketIndex < TotalPacketNum)
                   {
                       // Check sum
                       unsigned char datapkt[128];
                       unsigned char datachk = 0;
                       unsigned char Locdatachk = 0;

                       PacketCount++;
                       memset(strbuf, 0x00, sizeof(strbuf));
                       snprintf(strbuf, sizeof(strbuf), "Packet#:%d\r\n", PacketCount);
                       FileWrite(iHandle, (const void*)strbuf, strlen(strbuf));

                       datachk = _DataImage[(DataIndex+PacketAddr+(PacketDataSize-1))];
                       memcpy(datapkt, &_DataImage[(DataIndex+PacketAddr)], (PacketDataSize-1));

                       Locdatachk = u1Locus_Gen_Checksum(datapkt,(PacketDataSize-1));
                       if ( datachk == Locdatachk)
                       {
                           // Capture data in each packet
                           memset(FileBuffer, 0x00, sizeof(FileBuffer));
                           Locus_Capture_Data( FileBuffer, NMEABuffer,(DataIndex+PacketAddr), &FileBufLength, &NMEABufLength);
                           // Save to File
                           FileWrite(iHandle, (const void*)FileBuffer, FileBufLength);
                           // Save to NMEA File
                           FileWrite(iHandleNMEA, (const void*)NMEABuffer, NMEABufLength);
                           memset(strbuf, 0x00, sizeof(strbuf));
                           snprintf(strbuf, sizeof(strbuf), "\n");
                           FileWrite(iHandle, (const void*)strbuf, strlen(strbuf));
                       }
                       else
                       {
                           memset(strbuf, 0x00, sizeof(strbuf));
                           snprintf(strbuf, sizeof(strbuf), "Data Checksum Err,Addr,0x%x,CHK(%x,%x) \r\n",(DataIndex+PacketAddr), datachk, Locdatachk );
                           FileWrite(iHandle, (const void*)strbuf, strlen(strbuf));
                       }
                       PacketAddr = PacketAddr + PacketDataSize;
                       PacketIndex++;
                   }
               }
               else
               {
                   memset(strbuf, 0x00, sizeof(strbuf));
                   snprintf(strbuf, sizeof(strbuf), "Checksum Err,Addr,0x%x\r\n",DataIndex);
                   FileWrite(iHandle, (const void*)strbuf, strlen(strbuf));
               }
           }
        }
        else
        {
            //printf("Open files,fail,NMEA,%d,Log,%d",iHandleNMEA,iHandle);
            ret = false;
        }
        FileClose(iHandle);
        FileClose(iHandleNMEA);
    }
    else
    {
        // printf("Create files,fail,NMEA,%d,Log,%d",iHandleNMEA,iHandle);
        ret = false;
        FileClose(iHandleNMEA);
        FileClose(iHandle);
    }

    return ret;
 }
/* ------------------------------------------------------------
uLoadImage : Load the original Core file
---------------------------------------------------------------*/
unsigned uLoadImage ()
{

    int iHandle = FileOpen(_sInputDATAName, fmOpenRead | fmShareDenyNone);
    if (iHandle >= 0)
    {
        int iFileLength = FileSeek(iHandle, 0, 2);
        int iReadLength = iFileLength;

        if (NULL == _DataImage)
        {
            _DataImage = new char[iFileLength];
        }
        memset(_DataImage, 0xFF, iFileLength);
        _uDataSize = iFileLength;
        FileSeek(iHandle, 0, 0);


        iReadLength = FileRead(iHandle, (void*)&_DataImage[0], (unsigned)iFileLength);
        FileClose(iHandle);

       // printf("Loaded file %s size= %d \n", _sInputDATAName, iReadLength);
        return  (unsigned)iReadLength;
    }
    else
    {
       // printf("Fail to open bin file : %s\n", _sInputDATAName);
        return  0;
    }
}

int Locus_Parser_Start(char* InputDataFileName)
{
    int  ret = 0;
    int  I_DATAName = 0;   
    if (InputDataFileName != NULL)
    {

        I_DATAName =  AnsiString(InputDataFileName).Pos(".bin");
        if ( I_DATAName == 0)
        {
           // printf("The type of files is worng !\n");
            ret = 1;
        }
        else
        {
            _sInputDATAName = AnsiString(InputDataFileName);
            _sOutputLogName  = AnsiString( AnsiString(ExtractFilePath(InputDataFileName)) +AnsiString("Locus.loc") );
            _sOutputNMEAName =  AnsiString( AnsiString(ExtractFilePath(InputDataFileName)) +AnsiString("Locus.nma") );
            // printf("Input:%s\n", _sInputDATAName);
            // printf("Output:%s\n", _sOutputLogName);
            if (uLoadImage() != 0)
            {
                if (Locus_Parser() != true)
                {
                   ret = 2;
                }
            }
            else
            {
                ret = 4;
            }
           // printf("Done!\n");
        }
    }
    else
    {
        ret = 3;
    }
    free(_DataImage);
    _DataImage = NULL;
    return ret;
}
//---------------------------------------------------------------------------
