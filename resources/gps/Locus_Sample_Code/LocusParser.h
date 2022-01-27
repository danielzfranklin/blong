//---------------------------------------------
// Locus_Parser_Start
// Function : Parse Locus' data with binary format
// Parameter:
//    InputDataFileName : The file name of
//                 Locus' binary data
// Return   : 0 :success
//            1 : The type of files is worng
//            2 : Fail  to Create/Open output file
//            3 : File name is NULL
//            4 : Fail to read locus.bin
//---------------------------------------------
int Locus_Parser_Start(char* InputDataFileName);
