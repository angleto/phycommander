#include "phybroker_cli.h"
#include <boost/interprocess/managed_shared_memory.hpp>
#include <cstdlib> //std::system
#include <sstream>

PhybrokerCli::PhybrokerCli() {
        boost::interprocess::shared_memory_object
            phybroker_out (boost::interprocess::open_only //only create
                     ,"phycmd_outdata" //name
                     ,boost::interprocess::read_write //read-write mode
                     );

        boost::interprocess::shared_memory_object
            phybroker_in (boost::interprocess::open_only //only create
                 ,"phycmd_indata" //name
                 ,boost::interprocess::read_only //read-only mode
                 );
    }
    
PhybrokerCli::~PhybrokerCli() {
    
}

