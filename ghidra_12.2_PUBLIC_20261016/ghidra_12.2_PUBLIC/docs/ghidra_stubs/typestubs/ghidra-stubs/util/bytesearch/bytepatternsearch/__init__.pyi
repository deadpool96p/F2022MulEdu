from __future__ import annotations
import collections.abc
import datetime
import typing
from warnings import deprecated # type: ignore

import jpype # type: ignore
import jpype.protocol # type: ignore

import ghidra.program.model.lang
import ghidra.util.task
import java.io # type: ignore
import java.lang # type: ignore
import java.nio # type: ignore
import java.util # type: ignore
import org.xml.sax # type: ignore


@typing.type_check_only
class BpsConfig(java.lang.Object):
    """
    This class handles search configurations for Ghidra's Byte Pattern Searcher which dictate the
    types of data manipulations performed by :obj:`BpsTransformationEngine` required prior to search
    submission.
    
    
    The design of the Byte pattern Searcher and its supporting sub-capabilities and classes can be
    found in the "Search For Byte Patterns" page in the help viewer.
    
    
    Supported data manipulations are found in :obj:`BpsTransformationEngine`.
    
    
    Search configurations determine byte preparation manipulations prior to search submission.
    """

    class_: typing.ClassVar[java.lang.Class]

    def __init__(self, programWordSize: typing.Union[jpype.JInt, int], isByteExtension: typing.Union[jpype.JBoolean, bool], isSearchBE: typing.Union[jpype.JBoolean, bool], isSearchLE: typing.Union[jpype.JBoolean, bool]) -> None:
        """
        Constructor for handling search settings for patterns greater than 8 bits in size.
        
        :param jpype.JInt or int programWordSize: word size of the program
        :param jpype.JBoolean or bool isByteExtension: choice to zero-extend patterns to fit the word size of the program
        :param jpype.JBoolean or bool isSearchBE: choice to perform Big Endian search
        :param jpype.JBoolean or bool isSearchLE: choice to perform Little Endian search
        """


class BpsConfigBuilder(java.lang.Object):
    """
    A helper class to build up the potentially complicated :obj:`BpsConfig`.
    
    
    Manage the generation of a search configuration for use by Ghidra's Byte Pattern Search
    capability.
    
    
    The design of the Byte pattern Searcher and its supporting sub-capabilities and classes can be
    found in the "Search For Byte Patterns" page in the help viewer.
    """

    class_: typing.ClassVar[java.lang.Class]

    def __init__(self, programWordSize: typing.Union[jpype.JInt, int]) -> None:
        """
        Constructor.
        
        :param jpype.JInt or int programWordSize: the word size of the program; must be greater than 0
        """

    def build(self) -> BpsConfig:
        """
        Builds the final :obj:`BpsConfig`.
        
        :return: the new search configuration
        :rtype: BpsConfig
        """

    def setBigEndianSearch(self, isBeSearch: typing.Union[jpype.JBoolean, bool]) -> BpsConfigBuilder:
        """
        Specify whether to search for patterns as Big Endian. Default is true.
        
        :param jpype.JBoolean or bool isBeSearch: true signals patterns to be searched as BE (their assumed default
                endianness)
        :return: this builder instance
        :rtype: BpsConfigBuilder
        """

    def setLittleEndianSearch(self, isLeSearch: typing.Union[jpype.JBoolean, bool]) -> BpsConfigBuilder:
        """
        Specify whether to search for patterns as Little Endian.
        
        :param jpype.JBoolean or bool isLeSearch: true signals the transformation of patterns to be LE prior to
                search
        :return: this builder instance
        :rtype: BpsConfigBuilder
        """

    def setPerformExtension(self, isByteExtension: typing.Union[jpype.JBoolean, bool]) -> BpsConfigBuilder:
        """
        Specify whether to perform a zero-extension of byte patterns to fit the program word size if
        the pattern's word size is smaller than the program's.
        
        
        A true setting will perform quantization of sizes to build out the pattern's word size
        iteratively until it fills the program's word size. See :obj:`BpsConfig` for details.
        
        :param jpype.JBoolean or bool isByteExtension: true signals to zero-extend byte patterns to fill program word size
        :return: this builder instance
        :rtype: BpsConfigBuilder
        """


class BpsPattern(java.lang.Object):
    """
    This class represents a byte pattern in support of the :obj:`BpsXmlParser` of the Byte Pattern
    Search capability within Ghidra.
    
    
    The design of the XML Parser and its supporting classes can be found in :obj:`BpsXmlParser`.
    
    
    The design of the Byte pattern Searcher and its supporting sub-capabilities and classes can be
    found in the "Search For Byte Patterns" page in the help viewer.
    
    
    This class represents a byte pattern object which is populated by :obj:`BpsXmlParser`. This
    object wraps a search byte array (inside a :obj:`ByteBuffer`), its parent :obj:`BpsSearchItem`,
    and its parsed XML tag name for use downstream.
    
    
    ----------------
    Object Variables
    ----------------
    
    
    * name
    Indicates how to handle a pattern's matching and result analysis. Supported Names:
        
        * ``<Bytes>``– The default tag. Indicates a standard constant, function, or table
        search.
        * ``<Blacklist>``– Unique to function-based searches. Enforces filtering rules.
    
    The pattern's search type is indicated in the parent:obj:`BpsSearchItem`:
    OrderedFunction-type and Table-type searches will enforce the order of the
    buffer as presented in the pattern. See:obj:`BpsSearchType` for a complete discussion on search
    types.
    * buffer
    The byte array wrapped in a:obj:`ByteBuffer`. NOTE: all byte patterns must be in big Endian
    form.
    * parentSearchItem
    Links to the owning``<SearchItem>`` tag. This link is needed for result context
    preservation: when a match is found, the engine extracts metadata from the parent
    :obj:`BpsSearchItem` to populate the displayed results. Without this parental link, the results
    table will have empty gaps and lack critical context.
    * transformation
    The transformation details of how this pattern was formed; see:obj:`BpsTransformation` for
    details.
    """

    class_: typing.ClassVar[java.lang.Class]

    def __init__(self, name: typing.Union[java.lang.String, str], rawBytes: java.nio.ByteBuffer, parentSearchItem: BpsSearchItem, transformationRecord: BpsTransformation) -> None:
        """
        Constructor.
        
        :param java.lang.String or str name: parsed XML tag containing this byte pattern array
        :param java.nio.ByteBuffer rawBytes: parsed byte array wrapped in a ByteBuffer
        :param BpsSearchItem parentSearchItem: parent search item containing this pattern for searching
        :param BpsTransformation transformationRecord: byte manipulation order
        """

    def getBuffer(self) -> java.nio.ByteBuffer:
        """
        :return: the buffer containing the byte pattern. Downstream consumers are prevented from
        corrupting the pattern
        :rtype: java.nio.ByteBuffer
        """

    def getDescription(self) -> str:
        """
        :return: the description of the pattern
        :rtype: str
        """

    def getName(self) -> str:
        """
        :return: the name as parsed from the XML
        :rtype: str
        """

    def getSearchItem(self) -> BpsSearchItem:
        """
        :return: the parent search item containing this byte array
        :rtype: BpsSearchItem
        """

    def getTransformation(self) -> BpsTransformation:
        """
        :return: the transformation record of byte manipulations performed on the pattern
        :rtype: BpsTransformation
        """

    @property
    def name(self) -> java.lang.String:
        ...

    @property
    def searchItem(self) -> BpsSearchItem:
        ...

    @property
    def description(self) -> java.lang.String:
        ...

    @property
    def buffer(self) -> java.nio.ByteBuffer:
        ...

    @property
    def transformation(self) -> BpsTransformation:
        ...


class BpsSearchItem(java.lang.Object):
    """
    This class represents a parsed search item as used by the Byte Pattern Searcher capability within
    Ghidra.
    
    
    The design of the XML Parser and its supporting classes can be found in :obj:`BpsXmlParser`.
    
    
    The design of the Byte Pattern Searcher and its supporting sub-capabilities and classes can be
    found in the "Search For Byte Patterns" page in the help viewer.
    
    
    This class manages the search item characteristics parsed from the ``<SearchItem>`` XML tag.
    
    
    ---------------------------------------
    Byte Pattern Search Item Configurations
    ---------------------------------------
    
    
    
    **Required Variables:**
    
    * patternName – Name of the pattern for referencing.
    * wordSize – The pattern word size, resolved from the WordSize attribute from the
    ``<SearchItem>`` tag.
    * searchType – The :obj:`BpsSearchType` strategy (table, constant, function, or ordered
    function search, see:obj:`BpsSearchType`) which dictates how patterns are searched.
    * patterns – The ``<BpsPattern>`` collection of search byte sequences.
    
    
    
    
    **Optional Variables:**
    
    * description – Text explanation detailing the purpose of the search pattern.
    * submitter – The contributor who added this pattern to the :obj:`BpsSearch`.
    
    **Note:** additional attributes may be included on the ``<Search>`` tag, they
    will be stored in a map for filtering and sorting downstream.
    """

    class_: typing.ClassVar[java.lang.Class]

    def __init__(self, patternName: typing.Union[java.lang.String, str], key: typing.Union[java.lang.String, str], wordSize: typing.Union[jpype.JInt, int], searchType: BpsSearchType) -> None:
        """
        Constructor.
        
        :param java.lang.String or str patternName: name of pattern
        :param java.lang.String or str key: unique key to facilitate skipping patterns at the ``<SearchItem>`` level
        :param jpype.JInt or int wordSize: word size for pattern bytes (e.g. byte-8, word-16, dword-32, qword-64)
        :param BpsSearchType searchType: how the bytes should be searched
        """

    def getAdditionalAttributes(self) -> java.util.Map[java.lang.String, java.lang.String]:
        """
        :return: the additional attributes map parsed from the ``<SearchItem>`` tag
        :rtype: java.util.Map[java.lang.String, java.lang.String]
        """

    def getDescription(self) -> str:
        """
        :return: the search item's description
        :rtype: str
        """

    def getId(self) -> str:
        """
        :return: the search item's ID
        :rtype: str
        """

    def getPatternName(self) -> str:
        """
        :return: the pattern name
        :rtype: str
        """

    def getPatterns(self) -> java.util.List[BpsPattern]:
        """
        :return: the pattern's byte collection
        :rtype: java.util.List[BpsPattern]
        """

    def getSearchType(self) -> BpsSearchType:
        """
        :return: the pattern's search type
        :rtype: BpsSearchType
        """

    def getSubmitter(self) -> str:
        """
        :return: the submitter of the pattern
        :rtype: str
        """

    def getWordSize(self) -> int:
        """
        :return: the pattern's word size
        :rtype: int
        """

    @property
    def submitter(self) -> java.lang.String:
        ...

    @property
    def searchType(self) -> BpsSearchType:
        ...

    @property
    def patternName(self) -> java.lang.String:
        ...

    @property
    def patterns(self) -> java.util.List[BpsPattern]:
        ...

    @property
    def description(self) -> java.lang.String:
        ...

    @property
    def wordSize(self) -> jpype.JInt:
        ...

    @property
    def id(self) -> java.lang.String:
        ...

    @property
    def additionalAttributes(self) -> java.util.Map[java.lang.String, java.lang.String]:
        ...


class BpsTransformation(java.lang.Record):
    """
    This class records a byte pattern's transformation in support of Ghidra's Byte Pattern Search
    capability.
    
    
    The design of the Byte pattern Searcher and its supporting sub-capabilities and classes can be
    found in the "Search For Byte Patterns" page in the help viewer.
    
    
    Indicates the structural state mutations which have been applied to a :obj:`BpsPattern`.
    
    
    **NOTE**: Once patterns are generated, they do not change (instead, when transforming a
    pattern, a new pattern is generated and has its own transformation record) so pattern
    transformation records are not modifiable.
    """

    class_: typing.ClassVar[java.lang.Class]

    def __init__(self, endianness: ghidra.program.model.lang.Endian, extensionSize: typing.Union[java.lang.Integer, int], splitToSize: typing.Union[java.lang.Integer, int], splitPartIndex: typing.Union[java.lang.Integer, int], totalSplitParts: typing.Union[java.lang.Integer, int]) -> None:
        ...

    def asLittleEndian(self) -> BpsTransformation:
        """
        :return: Creates a copy of this record that uses the Little Endian setting
        :rtype: BpsTransformation
        """

    @staticmethod
    def empty() -> BpsTransformation:
        """
        Factory method for an unmodified baseline pattern.
        
        
        **NOTE**: All patterns prior to any transformations are safely assumed to be big endian.
        
        :return: the transformation
        :rtype: BpsTransformation
        """

    def endianness(self) -> ghidra.program.model.lang.Endian:
        ...

    def equals(self, o: java.lang.Object) -> bool:
        ...

    def extensionSize(self) -> int:
        ...

    def getDescription(self) -> str:
        """
        :return: the description of the pattern's transformation history
        :rtype: str
        """

    def hashCode(self) -> int:
        ...

    def splitPartIndex(self) -> int:
        ...

    def splitToSize(self) -> int:
        ...

    def totalSplitParts(self) -> int:
        ...

    @property
    def description(self) -> java.lang.String:
        ...


class BpsSearchType(java.lang.Enum[BpsSearchType]):
    """
    This enum represents the type of search to be conducted as used by the Byte Pattern Searcher
    capability within Ghidra.
    
    
    The design of the Byte Pattern Searcher and its supporting sub-capabilities and classes can be
    found in the "Search For Byte Patterns" page in the help viewer.
    
    
    This enum manages the possible supported search types.
    
    
    All :obj:`BpsSearchItem` objects will have a searchType which indicate how the
    :obj:`BpsPattern` items should be searched for. A complete discussion of all search types can be
    found in the "Search For Byte Patterns" page in the help viewer.
    """

    class_: typing.ClassVar[java.lang.Class]
    TABLE_SEARCH: typing.Final[BpsSearchType]
    """
    Table Search: byte patterns represent a table and must be found in the same order
    as in the pattern file
    """

    CONSTANT_SEARCH: typing.Final[BpsSearchType]
    """
    Constant Search: byte patterns to be found anywhere and in any order in the program.
    """

    FUNCTION_SEARCH: typing.Final[BpsSearchType]
    """
    Function Search: byte patterns must all be found within the same function.
    """

    ORDERED_FUNCTION_SEARCH: typing.Final[BpsSearchType]
    """
    Ordered Function Search: byte patterns must all be found within the same function 
    and also be found in the same order as in the pattern file.
    """


    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> BpsSearchType:
        ...

    @staticmethod
    def values() -> jpype.JArray[BpsSearchType]:
        ...


class BpsSearch(java.lang.Object):
    """
    This class represents a collection of :obj:`BpsSearchItem`'s and their corresponding attributes
    in support of Ghidra's Byte Pattern Searcher capability.
    
    
    The design of the XML Parser and its supporting classes, as well as an explanation of the
    required XML format can be found in :obj:`BpsXmlParser`.
    
    
    The design of the Byte Pattern Searcher and its supporting sub-capabilities and classes can be
    found in the "Search For Byte Patterns" page in the help viewer.
    
    
    This class manages a Search, or list, of :obj:`BpsSearchItem` objects parsed from the
    ``<SearchItem>`` tag. A search is populated by :obj:`BpsXmlParser` and referenced for
    processing by downstream Byte Pattern Searcher components.
    
    
    ----------------------------------
    Byte Pattern Search Configurations
    ----------------------------------
    
    
    
    **Required Variables:**
    
    
    
    * ``searchName``– Name of the search collection.
    
    
    
    
    **Optional Variables:**
    
    * ``description``– Text description of the pattern collection.
    * ``submitter``– Name of the pattern contributor.
    
    **Note:** additional attributes may be included on the ``<Search>`` tag, they
    will be stored in a map for filtering and sorting downstream.
    """

    class_: typing.ClassVar[java.lang.Class]

    def __init__(self, searchName: typing.Union[java.lang.String, str], generatedKey: typing.Union[java.lang.String, str], searchItems: java.util.List[BpsSearchItem]) -> None:
        """
        Constructor.
        
        :param java.lang.String or str searchName: name identifying a Search
        :param java.lang.String or str generatedKey: unique value across all BpsSearch and BpsSearchItem objects; used for
                skipping patterns at the ``<Search>`` and ``<SearchItem>`` levels.
        :param java.util.List[BpsSearchItem] searchItems: belonging to this Search
        """

    def getAdditionalAttributes(self) -> java.util.Map[java.lang.String, java.lang.String]:
        """
        :return: the additional attributes parsed from the ``<Search>`` tag
        :rtype: java.util.Map[java.lang.String, java.lang.String]
        """

    def getDescription(self) -> str:
        """
        :return: the description for the search
        :rtype: str
        """

    def getId(self) -> str:
        """
        :return: the search ID
        :rtype: str
        """

    def getSearchItems(self) -> java.util.List[BpsSearchItem]:
        """
        :return: the list of search items belonging to the Search
        :rtype: java.util.List[BpsSearchItem]
        """

    def getSearchName(self) -> str:
        """
        :return: the name of the Search
        :rtype: str
        """

    def getSubmitter(self) -> str:
        """
        :return: the submitter's identification
        :rtype: str
        """

    @property
    def submitter(self) -> java.lang.String:
        ...

    @property
    def searchName(self) -> java.lang.String:
        ...

    @property
    def description(self) -> java.lang.String:
        ...

    @property
    def id(self) -> java.lang.String:
        ...

    @property
    def searchItems(self) -> java.util.List[BpsSearchItem]:
        ...

    @property
    def additionalAttributes(self) -> java.util.Map[java.lang.String, java.lang.String]:
        ...


class BpsTransformationEngine(java.lang.Object):
    """
    This class performs byte pattern transformations in support of Ghidra's Byte Pattern Search
    capability.
    
    
    The design of the Byte pattern Searcher and its supporting sub-capabilities and classes can be
    found in the "Search For Byte Patterns" page in the help viewer.
    
    
    This transformer is used to prepare byte patterns parsed by the :obj:`BpsXmlParser` and
    populated in :obj:`BpsSearchItem` objects, prior to submitting them for search.
    
    
    Supported transformations are as follows:
    
    * Little Endian - Reverse a big Endian byte array. This is determined either
    programmatically based on the program's endianness or is user-controlled.
    * Byte Splitting - If the pattern's word size is larger than the program's word
    boundary, split the pattern to fit the boundary. This is determined programmatically and is not
    user-controlled.
    * Byte Zero-Padding - If the pattern's word size is smaller than the program's word
    boundary, zero-pad the byte pattern to fit the program's word boundary. The decision to extend
    bytes is user-controlled.
    
    
    
    A skip list is used to allow users to skip patterns at the :obj:`BpsSearch` and
    :obj:`BpsSearchItem` level.
    """

    class_: typing.ClassVar[java.lang.Class]

    def __init__(self) -> None:
        ...

    @typing.overload
    def createPatterns(self, searchLibrary: java.util.List[BpsSearch], config: BpsConfig) -> java.util.List[BpsPattern]:
        """
        Process an entire search library parsed by :obj:`BpsXmlParser` and populated in
        :obj:`BpsSearch` and :obj:`BpsSearchItem` objects in preparation for search submission.
        
        :param java.util.List[BpsSearch] searchLibrary: collection of search objects
        :param BpsConfig config: search preferences from user input and program details
        :return: All patterns prepared for search submission
        :rtype: java.util.List[BpsPattern]
        """

    @typing.overload
    def createPatterns(self, searchLibrary: java.util.List[BpsSearch], skipIds: java.util.Set[java.lang.String], config: BpsConfig) -> java.util.List[BpsPattern]:
        """
        Process an entire search library parsed by :obj:`BpsXmlParser` and populated in
        :obj:`BpsSearch` and :obj:`BpsSearchItem` objects in preparation for search submission.
        
        
        Skip over any patterns whose id's are included in a skip list.
        
        :param java.util.List[BpsSearch] searchLibrary: collection of search objects
        :param java.util.Set[java.lang.String] skipIds: list of :obj:`BpsSearch` and/or :obj:`BpsSearchItem` id's to skip over
        :param BpsConfig config: search preferences from user input and program details
        :return: All patterns prepared for search submission
        :rtype: java.util.List[BpsPattern]
        """


class BpsXmlParser(java.lang.Object):
    """
    This is the main class which parses pattern search XML files as used by the Byte Pattern Search
    capability within Ghidra.
    
    
    The design of the Byte pattern Searcher and its supporting sub-capabilities and classes can be
    found in the "Search For Byte Patterns" page in the help viewer.
    
    
    This class parses the provided byte pattern search XML files, populating corresponding Byte
    Pattern Searcher objects: :obj:`BpsSearch`,:obj:`BpsSearchItem`, and :obj:`BpsPattern` for
    processing and searching downstream.
    
    
    A search pattern XML file contains a set of byte patterns, organized into a simple hierarchy,
    which contain specific meta-data details that will shape the search criteria. A full discussion
    on the required XML schema can be found in the "Search For Byte Patterns" page in the help
    viewer.
    
    
    **Note:** All XML search documents must adhere to the structure outlined in the Byte
    Pattern Search help file. Parse errors due to malformed XML or an XML structure that does not
    follow the form, will result in accumulated parse errors which will be displayed once parsing has
    completed.
    """

    class_: typing.ClassVar[java.lang.Class]

    def __init__(self) -> None:
        ...

    @staticmethod
    def parseSearchFile(searchFile: jpype.protocol.SupportsPath, monitor: ghidra.util.task.TaskMonitor) -> java.util.List[BpsSearch]:
        """
        Parse the provided XML file, generating a list of :obj:`BpsSearch` objects. A map is used to
        support the skipping of patterns at the ``<Search>`` and ``<SearchItem>`` levels.
        
        
        All pattern files must have surrounding ``<BytePatternSearchLibrary>`` tags and adhere to
        the structure outlined in the Byte pattern Search help file.
        
        :param jpype.protocol.SupportsPath searchFile: XML file to be parsed
        :param ghidra.util.task.TaskMonitor monitor: the task monitor
        :return: Byte pattern searches
        :rtype: java.util.List[BpsSearch]
        :raises SAXException: for malformed XML parse errors
        """


@typing.type_check_only
class AccumulatingErrorHandler(org.xml.sax.ErrorHandler):
    """
    Custom ErrorHandler for XML parsing which keeps track of errors allowing parsing to complete
    instead of breaking on minor infractions in the XML.
    """

    class_: typing.ClassVar[java.lang.Class]

    def foundErrors(self) -> bool:
        """
        :return: the error status of the parser
        :rtype: bool
        """

    def getExceptions(self) -> java.util.List[org.xml.sax.SAXParseException]:
        """
        :return: the list of exceptions found during parsing
        :rtype: java.util.List[org.xml.sax.SAXParseException]
        """

    @property
    def exceptions(self) -> java.util.List[org.xml.sax.SAXParseException]:
        ...


@typing.type_check_only
class BpsXmlValidationException(org.xml.sax.SAXException):
    """
    Custom exception class adds a field for holding collected errors for ease of access and testing
    downstream.
    """

    class_: typing.ClassVar[java.lang.Class]



__all__ = ["BpsConfig", "BpsConfigBuilder", "BpsPattern", "BpsSearchItem", "BpsTransformation", "BpsSearchType", "BpsSearch", "BpsTransformationEngine", "BpsXmlParser", "AccumulatingErrorHandler", "BpsXmlValidationException"]
