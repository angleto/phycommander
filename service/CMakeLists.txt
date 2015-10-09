project("DicomServer")

cmake_minimum_required(VERSION 2.8)


IF(APPLE)
    CMAKE_POLICY(SET CMP0042 NEW)
ENDIF(APPLE)

SET(CMAKE_INSTALL_RPATH_USE_LINK_PATH TRUE)

find_package(PkgConfig REQUIRED)

SET( LINKING_MODE SHARED)
#SET( LINKING_MODE STATIC)

MESSAGE("Linking mode: ${LINKING_MODE}")

FIND_PACKAGE(Boost REQUIRED)
FIND_PACKAGE(Boost COMPONENTS regex REQUIRED)
FIND_PACKAGE(Boost COMPONENTS program_options REQUIRED)
FIND_PACKAGE(Boost COMPONENTS filesystem REQUIRED)
#FIND_PACKAGE(Boost COMPONENTS gil REQUIRED)
#FIND_PACKAGE(Boost COMPONENTS thread REQUIRED)
FIND_PACKAGE(Boost COMPONENTS system REQUIRED)
#FIND_PACKAGE(Boost COMPONENTS iostreams REQUIRED)

##FindOpenMP()
#FIND_PACKAGE( OpenMP REQUIRED )
#IF(OPENMP_FOUND)
#        MESSAGE("OPENMP FOUND")
#        SET(CMAKE_C_FLAGS "${CMAKE_C_FLAGS} ${OpenMP_C_FLAGS}")
#        SET(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} ${OpenMP_CXX_FLAGS}")
#        SET(CMAKE_EXE_LINKER_FLAGS "${CMAKE_EXE_LINKER_FLAGS} ${OpenMP_EXE_LINKER_FLAGS}")
#ENDIF()

include_directories(
        "${Boost_INCLUDE_DIRS}"
)

link_directories(
        "/usr/local/lib"
)

add_definitions(
	-std=c++11
)

IF(NOT APPLE)
	add_definitions(
		#"-Wstatic-float-init"
		#"-fpermissive"
		)
ENDIF()

SUBDIRS(
	phybroker
)

