from __future__ import annotations
import collections.abc
import datetime
import typing
from warnings import deprecated # type: ignore

import jpype # type: ignore
import jpype.protocol # type: ignore

import docking
import docking.widgets.autocomplete
import ghidra.app.plugin.assembler
import ghidra.program.model.address
import ghidra.program.model.lang
import ghidra.program.model.listing
import java.awt # type: ignore
import java.lang # type: ignore
import javax.swing # type: ignore


class AssemblyCompletion(java.lang.Comparable[AssemblyCompletion]):
    """
    A generic class for all items listed by the autocompleter
    """

    class_: typing.ClassVar[java.lang.Class]

    def __init__(self, text: typing.Union[java.lang.String, str], display: typing.Union[java.lang.String, str], color: java.awt.Color, order: typing.Union[jpype.JInt, int]) -> None:
        ...

    def getCanDefault(self) -> bool:
        """
        Override this to permit activation by default, i.e., on CTRL-SPACE
        
        :return: true to permit defaulting, false to prevent it
        :rtype: bool
        """

    def getColor(self) -> java.awt.Color:
        """
        Get the foreground color for the item
        
        :return: the color
        :rtype: java.awt.Color
        """

    def getDisplay(self) -> str:
        """
        Get the (possibly HTML) text to display for the item
        
        :return: the text
        :rtype: str
        """

    def getText(self) -> str:
        """
        Get the text to insert when the item is activated
        
        :return: the text
        :rtype: str
        """

    @property
    def canDefault(self) -> jpype.JBoolean:
        ...

    @property
    def color(self) -> java.awt.Color:
        ...

    @property
    def display(self) -> java.lang.String:
        ...

    @property
    def text(self) -> java.lang.String:
        ...


class InstructionAssemblyCompletion(AssemblyCompletion):
    """
    Represents an encoding for a complete assembly instruction
    
    
    
    These provide no insertion text, since their activation should be handled by a custom listener.
    """

    class_: typing.ClassVar[java.lang.Class]

    def __init__(self, program: ghidra.program.model.listing.Program, language: ghidra.program.model.lang.Language, at: ghidra.program.model.address.Address, text: typing.Union[java.lang.String, str], data: jpype.JArray[jpype.JByte], ctxVal: ghidra.program.model.lang.RegisterValue, preference: typing.Union[jpype.JInt, int]) -> None:
        ...

    def getData(self) -> jpype.JArray[jpype.JByte]:
        """
        Get the assembled instruction bytes
        
        :return: the bytes
        :rtype: jpype.JArray[jpype.JByte]
        """

    @property
    def data(self) -> jpype.JArray[jpype.JByte]:
        ...


class ContextChanges(ghidra.program.model.lang.DisassemblerContextAdapter):

    class_: typing.ClassVar[java.lang.Class]

    def __init__(self, contextIn: ghidra.program.model.lang.RegisterValue) -> None:
        ...

    def addFlow(self, progCtx: ghidra.program.model.listing.ProgramContext, after: ghidra.program.model.address.Address) -> None:
        ...


class AssemblyAutocompletionModel(docking.widgets.autocomplete.AutocompletionModel[AssemblyCompletion]):

    class_: typing.ClassVar[java.lang.Class]

    def __init__(self) -> None:
        ...

    def getAssembler(self) -> ghidra.app.plugin.assembler.Assembler:
        """
        :return: the assembler used by this model
        :rtype: ghidra.app.plugin.assembler.Assembler
        """

    def setAddress(self, address: ghidra.program.model.address.Address) -> None:
        """
        Set the address of the assembly instruction
        
        
        Note this will reset the existing instruction to null to prevent its accidental re-use. See
        :meth:`setExisting(Instruction) <.setExisting>`.
        
        :param ghidra.program.model.address.Address address: the address
        """

    def setAssembler(self, assembler: ghidra.app.plugin.assembler.Assembler) -> None:
        """
        Set the assembler to use
        
        :param ghidra.app.plugin.assembler.Assembler assembler: the assembler
        """

    def setExisting(self, existing: ghidra.program.model.listing.Instruction) -> None:
        """
        Set the "existing" instruction used for ordering proposed instructions by "most similar"
        
        :param ghidra.program.model.listing.Instruction existing: the existing instruction
        
        .. seealso::
        
            | :obj:`.computePreference(AssemblyResolvedPatterns)`
        """

    @property
    def assembler(self) -> ghidra.app.plugin.assembler.Assembler:
        ...

    @assembler.setter
    def assembler(self, value: ghidra.app.plugin.assembler.Assembler):
        ...


class AssemblyAutocompleter(docking.widgets.autocomplete.TextComponentAutocompleter[AssemblyCompletion], docking.widgets.autocomplete.AutocompletionListener[AssemblyCompletion]):
    """
    A customized autocompleter for assembly
    
    
    
    This positions the list at the bottom left of the field(s), and considers the full text of the
    linked text boxes when retrieving the prefix. It also delegates the item styling to the item
    instances.
    """

    class_: typing.ClassVar[java.lang.Class]

    def __init__(self, model: AssemblyAutocompletionModel) -> None:
        ...

    def fakeFocusGained(self, field: javax.swing.JTextField) -> None:
        ...

    def getExhaustButton(self) -> docking.EmptyBorderToggleButton:
        ...

    @property
    def exhaustButton(self) -> docking.EmptyBorderToggleButton:
        ...


class ErrorAssemblyCompletion(AssemblyCompletion):
    """
    Represents the description of an error encountered during parsing or assembling
    
    
    
    **NOTE:** not used until error descriptions improve
    """

    class_: typing.ClassVar[java.lang.Class]

    def __init__(self, text: typing.Union[java.lang.String, str], desc: typing.Union[java.lang.String, str]) -> None:
        ...


class SuggestionAssemblyCompletion(AssemblyCompletion):
    """
    Represents a textual suggestion to complete or partially complete an assembly instruction
    """

    class_: typing.ClassVar[java.lang.Class]

    def __init__(self, text: typing.Union[java.lang.String, str], display: typing.Union[java.lang.String, str]) -> None:
        ...



__all__ = ["AssemblyCompletion", "InstructionAssemblyCompletion", "ContextChanges", "AssemblyAutocompletionModel", "AssemblyAutocompleter", "ErrorAssemblyCompletion", "SuggestionAssemblyCompletion"]
