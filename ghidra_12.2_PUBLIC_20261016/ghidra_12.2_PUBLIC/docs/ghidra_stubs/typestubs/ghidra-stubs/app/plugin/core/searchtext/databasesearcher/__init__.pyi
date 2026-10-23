from __future__ import annotations
import collections.abc
import datetime
import typing
from warnings import deprecated # type: ignore

import jpype # type: ignore
import jpype.protocol # type: ignore

import ghidra.app.plugin.core.searchtext
import ghidra.framework.plugintool
import ghidra.program.model.address
import ghidra.program.model.listing
import ghidra.program.util
import ghidra.util.task
import java.lang # type: ignore
import java.util.regex # type: ignore


class CommentAddressSupplier(java.lang.Object):
    """
    Shared supplier of comments at addresses as they are traversed in address order (or reverse
    address order). 
    
    
    This object is used as a part of a larger search operation and is shared by the various comment
    searchers for efficiency since all comment types are actually stored in the same database
    record.
    
    
    At any given time, this supplier has a current address, which represents the next address at
    or beyond the overall search operation's address. When asked to advance, a current search
    address is passed, which is the last address that has been fully searched. So if our current
    address is at that address, we need to advance our current address to the next comment address.
    Otherwise, we simply return our current address which the higher level search can use
    to determine the next overall search address.
    """

    class_: typing.ClassVar[java.lang.Class]

    def __init__(self, program: ghidra.program.model.listing.Program, addresses: ghidra.program.model.address.AddressSetView, forward: typing.Union[jpype.JBoolean, bool]) -> None:
        ...

    def advance(self, address: ghidra.program.model.address.Address) -> ghidra.program.model.address.Address:
        """
        Advance the current address to the next address that contains any type of comment if the 
        passed in address is null or equal to the current address. The idea is that we are part
        of a larger search operation that is marching through the address space. We only want to 
        advance our current address if the overall address of the search matches our address.
        (meaning we have already served up comments for the given address so we can advance to the
        next address that has comments, but may not be the next address of the overall search
        operation is considering.)
        
        :param ghidra.program.model.address.Address address: the address that has been already processed, so we need to make sure our
        current address is past this address.
        :return: an address that is past the given address. Could be our current address our 
        address hasn't been reached yet, or we search forward to the next address containing a
        comment.
        :rtype: ghidra.program.model.address.Address
        """

    def getCurrentAddress(self) -> ghidra.program.model.address.Address:
        """
        :return: the address of the currently available comments
        :rtype: ghidra.program.model.address.Address
        """

    def getCurrentComment(self, type: ghidra.program.model.listing.CommentType) -> str:
        """
        Returns the comment of the specified type at the current address or null if no comment of
        that type exists at the current address.
        
        :param ghidra.program.model.listing.CommentType type: the type of comment to retrieve
        :return: the comment of the specified type at the current address
        :rtype: str
        """

    @property
    def currentComment(self) -> java.lang.String:
        ...

    @property
    def currentAddress(self) -> ghidra.program.model.address.Address:
        ...


class CommentFieldSearcher(ProgramDatabaseFieldSearcher):
    """
    Field searcher for comments. Since all comments types for an address are stored in the same
    database record, comment searchers share a common :obj:`CommentAddressSupplier` which the
    overall searcher will manually advance. The advance here will just assume the the comment
    supplier has been advanced and it will just return the suppliers current address.
    """

    class_: typing.ClassVar[java.lang.Class]

    def __init__(self, supplier: CommentAddressSupplier, program: ghidra.program.model.listing.Program, startLoc: ghidra.program.util.ProgramLocation, set: ghidra.program.model.address.AddressSetView, forward: typing.Union[jpype.JBoolean, bool], pattern: java.util.regex.Pattern, commentType: ghidra.program.model.listing.CommentType) -> None:
        ...


class ProgramDatabaseFieldSearcher(java.lang.Object):

    class_: typing.ClassVar[java.lang.Class]

    def getMatch(self) -> ghidra.app.plugin.core.searchtext.Searcher.TextSearchResult:
        ...

    def getNextSignificantAddress(self, address: ghidra.program.model.address.Address) -> ghidra.program.model.address.Address:
        ...

    def hasMatch(self, address: ghidra.program.model.address.Address) -> bool:
        ...

    @property
    def match(self) -> ghidra.app.plugin.core.searchtext.Searcher.TextSearchResult:
        ...

    @property
    def nextSignificantAddress(self) -> ghidra.program.model.address.Address:
        ...


class DataMnemonicOperandFieldSearcher(ProgramDatabaseFieldSearcher):
    ...
    class_: typing.ClassVar[java.lang.Class]


class FunctionFieldSearcher(ProgramDatabaseFieldSearcher):

    class_: typing.ClassVar[java.lang.Class]

    def __init__(self, program: ghidra.program.model.listing.Program, startLoc: ghidra.program.util.ProgramLocation, set: ghidra.program.model.address.AddressSetView, forward: typing.Union[jpype.JBoolean, bool], pattern: java.util.regex.Pattern) -> None:
        ...


class InstructionMnemonicOperandFieldSearcher(ProgramDatabaseFieldSearcher):

    class_: typing.ClassVar[java.lang.Class]

    @staticmethod
    def createInstructionMnemonicAndOperandFieldSearcher(program: ghidra.program.model.listing.Program, startLoc: ghidra.program.util.ProgramLocation, set: ghidra.program.model.address.AddressSetView, forward: typing.Union[jpype.JBoolean, bool], pattern: java.util.regex.Pattern, format: ghidra.program.model.listing.CodeUnitFormat) -> InstructionMnemonicOperandFieldSearcher:
        ...

    @staticmethod
    def createInstructionMnemonicOnlyFieldSearcher(program: ghidra.program.model.listing.Program, startLoc: ghidra.program.util.ProgramLocation, set: ghidra.program.model.address.AddressSetView, forward: typing.Union[jpype.JBoolean, bool], pattern: java.util.regex.Pattern, format: ghidra.program.model.listing.CodeUnitFormat) -> InstructionMnemonicOperandFieldSearcher:
        ...

    @staticmethod
    def createInstructionOperandOnlyFieldSearcher(program: ghidra.program.model.listing.Program, startLoc: ghidra.program.util.ProgramLocation, set: ghidra.program.model.address.AddressSetView, forward: typing.Union[jpype.JBoolean, bool], pattern: java.util.regex.Pattern, format: ghidra.program.model.listing.CodeUnitFormat) -> InstructionMnemonicOperandFieldSearcher:
        ...


class LabelFieldSearcher(ProgramDatabaseFieldSearcher):

    @typing.type_check_only
    class SymbolAddressIterator(ghidra.program.model.address.AddressIterator):
        ...
        class_: typing.ClassVar[java.lang.Class]


    class_: typing.ClassVar[java.lang.Class]

    def __init__(self, program: ghidra.program.model.listing.Program, startLoc: ghidra.program.util.ProgramLocation, set: ghidra.program.model.address.AddressSetView, forward: typing.Union[jpype.JBoolean, bool], pattern: java.util.regex.Pattern) -> None:
        ...


class ProgramDatabaseSearchTableModel(ghidra.app.plugin.core.searchtext.AbstractSearchTableModel):
    """
    Table model for showing results of "Search All" in a program database Text search.
    """

    class_: typing.ClassVar[java.lang.Class]

    def __init__(self, tool: ghidra.framework.plugintool.PluginTool, p: ghidra.program.model.listing.Program, set: ghidra.program.model.address.AddressSetView, options: ghidra.app.plugin.core.searchtext.SearchOptions) -> None:
        """
        Constructs a program database text search table model.
        
        :param ghidra.framework.plugintool.PluginTool tool: the tool
        :param ghidra.program.model.listing.Program p: the program
        :param ghidra.program.model.address.AddressSetView set: address set to search
        :param ghidra.app.plugin.core.searchtext.SearchOptions options: search options
        """


class ProgramDatabaseSearcher(ghidra.app.plugin.core.searchtext.Searcher):
    """
    This class combines multiple field searchers to present a simple searcher interface for users of 
    this class. First, based on the searchOptions, a field searcher is created for each field to be
    search (comments, mnemonics, operands, etc.)  The searchers are ordered based on which field's matches
    should be presented before another field's match at the same address.  Backwards searches would
    have the searchers in the opposite order for forward searches.  This search is an efficient breadth
    first search by requiring that each searcher only advance one record and only move to the next record
    when all the other searches have reached records at or beyond the address of this searcher's current
    record.  The basic algorithm is to ask each searcher if they have a match at the current address.  Since
    they are asked in the appropriate order, if any of them has a match at the current address, it is
    immediately returned.  Once all the searchers report not having a match at the current address, the
    current address is advanced to the next address of the searcher whose current record is closest to
    the current address (if the searcher's record is the current address, this is when it would fetch
    its next record).
    
    
    When a searcher's getMatch() method is called, the searcher should return it current match and 
    advance its internal pointer to any additional matches at the same address or be prepared to 
    report no match when the hasMatch() method is called again at the same address.  When the 
    findNextSignificantAddress() method is called, the searcher should report its current record's
    address if that address is not the current address.  Otherwise, the searcher should advance to
    its next record and report that address.  When a search has no more records in the search
    address set, it should return null for the findNextSignificantAddress() method and hasMatch should
    return false.
    """

    class_: typing.ClassVar[java.lang.Class]

    def __init__(self, serviceProvider: ghidra.framework.plugintool.ServiceProvider, program: ghidra.program.model.listing.Program, startLoc: ghidra.program.util.ProgramLocation, set: ghidra.program.model.address.AddressSetView, options: ghidra.app.plugin.core.searchtext.SearchOptions, monitor: ghidra.util.task.TaskMonitor) -> None:
        ...



__all__ = ["CommentAddressSupplier", "CommentFieldSearcher", "ProgramDatabaseFieldSearcher", "DataMnemonicOperandFieldSearcher", "FunctionFieldSearcher", "InstructionMnemonicOperandFieldSearcher", "LabelFieldSearcher", "ProgramDatabaseSearchTableModel", "ProgramDatabaseSearcher"]
