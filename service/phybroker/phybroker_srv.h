#ifndef __PHYBROKER_SRV__
#define __PHYBROKER_SRV__

#include "common_declarations.h"
#include <boost/shared_ptr.hpp>
#include <boost/interprocess/managed_shared_memory.hpp>

class PhybrokerSrv {
    
public:
    PhybrokerSrv() ;
    
    char * getOutDataHandler() const ;
    char * getInDataHandler() const ;
    
    virtual ~PhybrokerSrv() ;
    
private:
    boost::shared_ptr<boost::interprocess::managed_shared_memory> phybroker_out ;
    boost::shared_ptr<boost::interprocess::managed_shared_memory> phybroker_in ;
    
    void * out_data_ptr ;
    void * in_data_ptr ;
    
    boost::interprocess::managed_shared_memory::handle_t out_handle ;
    boost::interprocess::managed_shared_memory::handle_t in_handle ;
};

#endif //__PHYBROKER_SRV__