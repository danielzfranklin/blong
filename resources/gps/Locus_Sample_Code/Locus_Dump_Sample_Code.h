typedef struct{
  unsigned short u2Type;     // PMTKLOX packet types: 0 - LOCUS start, 1 - LOCUS data, 2 - LOCUS end
  unsigned long u4Data[25];  // Store filed data in an integer
  unsigned short u2FieldNum; // How many number of fields in PMTKLOX (not include type field)
} NMEA_MLOX_T;
