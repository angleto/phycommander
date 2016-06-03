PROJECT(phybroker)

ADD_LIBRARY(phybroker_srv
    common_declarations.h
    phybroker_srv.cpp
    phybroker_srv.h
)

TARGET_LINK_LIBRARIES(phybroker_srv
    ${Boost_PROGRAM_OPTIONS_LIBRARY}
    ${Boost_FILESYSTEM_LIBRARY}
)

ADD_LIBRARY(phybroker_cli
    common_declarations.h
    phybroker_cli.cpp
    phybroker_cli.h
)

TARGET_LINK_LIBRARIES(phybroker_cli
${Boost_PROGRAM_OPTIONS_LIBRARY}
${Boost_FILESYSTEM_LIBRARY}
)

ADD_EXECUTABLE(physervice
    physervice.cpp
    physervice.h
)

TARGET_LINK_LIBRARIES(physervice
    ${Boost_PROGRAM_OPTIONS_LIBRARY}
    ${Boost_FILESYSTEM_LIBRARY}
    phybroker_srv
    pthread
    rt
)

ADD_EXECUTABLE(phyclient
    phyclient.cpp
    phyclient.h
)

TARGET_LINK_LIBRARIES(phyclient
    ${Boost_PROGRAM_OPTIONS_LIBRARY}
    ${Boost_FILESYSTEM_LIBRARY}
    phybroker_cli
    rt
)
