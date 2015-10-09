#include "phybroker_srv.h"
#include <boost/interprocess/managed_shared_memory.hpp>
#include <boost/interprocess/exceptions.hpp>
#include <cstdlib> //std::system
#include <sstream>
#include <iostream>

PhybrokerSrv::PhybrokerSrv() {

    boost::interprocess::shared_memory_object::remove("phycmd_outdata") ;
    boost::interprocess::shared_memory_object::remove("phycmd_indata") ;
    
    try {
        phybroker_out = boost::shared_ptr<boost::interprocess::managed_shared_memory>(new boost::interprocess::managed_shared_memory(boost::interprocess::create_only //only create
            ,"phycmd_outdata" //name
            ,out_data_size
        ));
    } catch (const boost::interprocess::interprocess_exception& e) {
        std::cout << "Fatal Error: " << e.get_native_error() << " : " << e.get_error_code() << " : " << e.what() << std::endl ;
        exit(100) ;
    }

    out_data_ptr = phybroker_out->allocate(out_data_size);
    out_handle = phybroker_out->get_handle_from_address(out_data_ptr);

    phybroker_in = boost::shared_ptr<boost::interprocess::managed_shared_memory>(new boost::interprocess::managed_shared_memory(boost::interprocess::create_only //only create
                          ,"phycmd_indata" //name
                          ,in_data_size
                        ));

    in_data_ptr = phybroker_in->allocate(out_data_size);
    in_handle = phybroker_in->get_handle_from_address(in_data_ptr);
}

char * PhybrokerSrv::getOutDataHandler() const {
    return (char *)out_data_ptr ;
}

char * PhybrokerSrv::getInDataHandler() const {
    return (char *)in_data_ptr ;
}

PhybrokerSrv::~PhybrokerSrv() {
        boost::interprocess::shared_memory_object::remove("phycmd_outdata") ;
        boost::interprocess::shared_memory_object::remove("phycmd_indata") ;
}


