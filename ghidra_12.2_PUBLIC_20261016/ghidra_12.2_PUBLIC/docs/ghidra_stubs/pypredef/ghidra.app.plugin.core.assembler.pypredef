from __future__ import annotations
import collections.abc
import datetime
import typing
from warnings import deprecated # type: ignore

import jpype # type: ignore
import jpype.protocol # type: ignore

import docking
import docking.action
import docking.widgets.autocomplete
import docking.widgets.table
import ghidra
import ghidra.app.plugin
import ghidra.app.plugin.assembler
import ghidra.app.plugin.assembler.sleigh.sem
import ghidra.app.plugin.core.assembler.completion
import ghidra.framework.cmd
import ghidra.framework.plugintool
import ghidra.program.model.address
import ghidra.program.model.lang
import ghidra.program.model.listing
import ghidra.util.table.column
import ghidra.util.task
import java.awt # type: ignore
import java.awt.event # type: ignore
import java.lang # type: ignore
import java.util # type: ignore
import javax.swing # type: ignore


T = typing.TypeVar("T")


class AbstractAssemblePatchDialog(docking.DialogComponentProvider, typing.Generic[T]):

    @typing.type_check_only
    class AssemblyRow(java.lang.Record):

        class_: typing.ClassVar[java.lang.Class]

        def address(self) -> ghidra.program.model.address.Address:
            ...

        def assembly(self) -> str:
            ...

        def bytes(self) -> jpype.JArray[jpype.JByte]:
            ...

        def context(self) -> ghidra.app.plugin.assembler.sleigh.sem.AssemblyPatternBlock:
            ...

        def equals(self, o: java.lang.Object) -> bool:
            ...

        def error(self) -> java.lang.Throwable:
            ...

        def hashCode(self) -> int:
            ...

        def toString(self) -> str:
            ...


    @typing.type_check_only
    class AsmTableModel(docking.widgets.table.GDynamicColumnTableModel[AbstractAssemblePatchDialog.AssemblyRow, java.lang.Void]):

        class_: typing.ClassVar[java.lang.Class]

        def __init__(self, serviceProvider: ghidra.framework.plugintool.ServiceProvider) -> None:
            ...

        def getLines(self) -> java.util.List[java.lang.String]:
            ...

        @property
        def lines(self) -> java.util.List[java.lang.String]:
            ...


    @typing.type_check_only
    class MonospacedRenderer(ghidra.util.table.column.AbstractGColumnRenderer[java.lang.Object]):
        ...
        class_: typing.ClassVar[java.lang.Class]


    @typing.type_check_only
    class AbstractAsmColumn(docking.widgets.table.AbstractDynamicTableColumn[AbstractAssemblePatchDialog.AssemblyRow, T, java.lang.Void], typing.Generic[T]):
        ...
        class_: typing.ClassVar[java.lang.Class]


    @typing.type_check_only
    class AsmAddressColumn(AbstractAssemblePatchDialog.AbstractAsmColumn[ghidra.program.model.address.Address]):
        ...
        class_: typing.ClassVar[java.lang.Class]


    @typing.type_check_only
    class AsmBytesColumn(AbstractAssemblePatchDialog.AbstractAsmColumn[java.lang.String]):
        ...
        class_: typing.ClassVar[java.lang.Class]


    @typing.type_check_only
    class AsmContextColumn(AbstractAssemblePatchDialog.AbstractAsmColumn[java.lang.String]):
        ...
        class_: typing.ClassVar[java.lang.Class]


    @typing.type_check_only
    class AssemblyLineRenderer(ghidra.util.table.column.AbstractGColumnRenderer[AbstractAssemblePatchDialog.AssemblyRow]):
        ...
        class_: typing.ClassVar[java.lang.Class]


    @typing.type_check_only
    class AsmErrorColumn(AbstractAssemblePatchDialog.AbstractAsmColumn[java.lang.String]):
        ...
        class_: typing.ClassVar[java.lang.Class]


    @typing.type_check_only
    class AsmAssemblyColumn(docking.widgets.table.AbstractDynamicTableColumn[AbstractAssemblePatchDialog.AssemblyRow, AbstractAssemblePatchDialog.AssemblyRow, java.lang.Void]):
        ...
        class_: typing.ClassVar[java.lang.Class]


    @typing.type_check_only
    class Highlighter(java.lang.Object):
        ...
        class_: typing.ClassVar[java.lang.Class]


    class_: typing.ClassVar[java.lang.Class]


class AbstractPatchAssemblyCommand(ghidra.framework.cmd.BackgroundCommand[T], typing.Generic[T]):

    class_: typing.ClassVar[java.lang.Class]

    @typing.overload
    def __init__(self, asm: ghidra.app.plugin.assembler.Assembler, lines: java.util.List[java.lang.String], entry: ghidra.program.model.address.Address, initialContext: ghidra.program.model.lang.RegisterValue) -> None:
        ...

    @typing.overload
    def __init__(self, asm: ghidra.app.plugin.assembler.Assembler, string: typing.Union[java.lang.String, str], entry: ghidra.program.model.address.Address, initialContext: ghidra.program.model.lang.RegisterValue) -> None:
        ...

    def assemble(self, program: T, monitor: ghidra.util.task.TaskMonitor) -> ghidra.program.model.address.AddressSetView:
        ...

    def getNext(self) -> ghidra.program.model.address.Address:
        ...

    def getSet(self) -> ghidra.program.model.address.AddressSetView:
        ...

    @property
    def next(self) -> ghidra.program.model.address.Address:
        ...

    @property
    def set(self) -> ghidra.program.model.address.AddressSetView:
        ...


class AbstractPatchAction(docking.action.DockingAction):
    """
    An abstract action for patching
    
    
    
    This handles most of the field placement, but relies on quite a few callbacks.
    """

    class_: typing.ClassVar[java.lang.Class]
    MENU_GROUP: typing.Final = "Disassembly"

    def __init__(self, owner: ghidra.framework.plugintool.Plugin, name: typing.Union[java.lang.String, str]) -> None:
        """
        Create a new action owned by the given plugin, having the given name
        
        :param ghidra.framework.plugintool.Plugin owner: the plugin owning the action
        :param java.lang.String or str name: the name of the action
        """

    def accept(self) -> None:
        """
        Invoked when the user presses Enter
        
        
        
        This should validate the user's input and complete the action. If the action is completed
        successfully, then call :meth:`hide() <.hide>`. Note that the Enter key can be ignored by doing
        nothing, since the input field(s) will remain visible. In that case, you must provide another
        mechanism for completing the action.
        """

    def cancel(self) -> None:
        """
        Cancel the current patch action
        
        
        
        This hides the input field(s) without completing the action.
        """


class AssemblePatchAction(docking.action.DockingAction):

    class_: typing.ClassVar[java.lang.Class]

    def __init__(self, plugin: ghidra.framework.plugintool.Plugin, name: typing.Union[java.lang.String, str]) -> None:
        ...


class AssemblePatchDialog(AbstractAssemblePatchDialog[ghidra.program.model.listing.Program]):
    ...
    class_: typing.ClassVar[java.lang.Class]


class AssemblerPlugin(ghidra.app.plugin.ProgramPlugin):
    """
    A plugin for assembly
    
    
    
    This plugin currently provides two actions: :obj:`PatchInstructionAction`, which allows the user
    to assemble an instruction at the current address; and :obj:`PatchDataAction`, which allows the
    user to "assemble" data at the current address.
    
    
    
    The API for instruction assembly is available from :obj:`Assemblers`. For data assembly, the API
    is in :meth:`DataType.encodeRepresentation(String, MemBuffer, Settings, int) <DataType.encodeRepresentation>`.
    """

    @typing.type_check_only
    class AssemblyRating(java.lang.Enum[AssemblerPlugin.AssemblyRating]):
        """
        Enumerated quality ratings and text to describe them.
        """

        class_: typing.ClassVar[java.lang.Class]
        UNRATED: typing.Final[AssemblerPlugin.AssemblyRating]
        POOR: typing.Final[AssemblerPlugin.AssemblyRating]
        BRONZE: typing.Final[AssemblerPlugin.AssemblyRating]
        SILVER: typing.Final[AssemblerPlugin.AssemblyRating]
        GOLD: typing.Final[AssemblerPlugin.AssemblyRating]
        PLATINUM: typing.Final[AssemblerPlugin.AssemblyRating]

        @staticmethod
        def valueOf(name: typing.Union[java.lang.String, str]) -> AssemblerPlugin.AssemblyRating:
            ...

        @staticmethod
        def values() -> jpype.JArray[AssemblerPlugin.AssemblyRating]:
            ...


    @typing.type_check_only
    class AssemblerConstructorWorker(ghidra.util.task.CachingSwingWorker[ghidra.app.plugin.assembler.Assembler]):

        class_: typing.ClassVar[java.lang.Class]

        def __init__(self, language: ghidra.program.model.lang.Language) -> None:
            ...


    class_: typing.ClassVar[java.lang.Class]
    ASSEMBLER_NAME: typing.Final = "Assembler"
    CACHE: typing.Final[java.util.Map[ghidra.program.model.lang.Language, ghidra.util.task.CachingSwingWorker[ghidra.app.plugin.assembler.Assembler]]]

    def __init__(self, tool: ghidra.framework.plugintool.PluginTool) -> None:
        ...


class AssemblyDualTextField(java.lang.Object):
    """
    A pair of text fields suitable for guided assembly
    
    
    
    This object must be updated with program location information, so that it knows the applicable
    language and address. It then provides two text boxes: one for the mnemonic, and one for the
    operands. The two are linked so that the user can intuitively navigate between them as if they
    were a single text box. The boxes are also attached to an autocompleter. It provides suggestions
    based syntax errors returned by the assembler. When a valid instruction is present, it provides
    the resulting instruction bytes.
    
    
    
    To detect when the user has activated an instruction-bytes entry, add an
    :obj:`AutocompletionListener` and check that the selection is an
    :obj:`InstructionAssemblyCompletion`. Otherwise, the usual autocompletion behavior is applied
    automatically.
    """

    @typing.type_check_only
    class EnterKeyListener(java.awt.event.KeyListener):
        """
        A listener which activates the autocompleter on ENTER (in addition to the default
        CTRL-SPACE).
        
        
        Because the user must activate an entry to specify the desired assembly, we make ENTER pull
        up the list, hinting that the user must make a selection.
        """

        class_: typing.ClassVar[java.lang.Class]


    @typing.type_check_only
    class MyAutocompleter(ghidra.app.plugin.core.assembler.completion.AssemblyAutocompleter):

        class_: typing.ClassVar[java.lang.Class]

        def __init__(self) -> None:
            ...


    @typing.type_check_only
    class AssemblyDualTextAutocompletionModel(ghidra.app.plugin.core.assembler.completion.AssemblyAutocompletionModel):
        ...
        class_: typing.ClassVar[java.lang.Class]


    class VisibilityMode(java.lang.Enum[AssemblyDualTextField.VisibilityMode]):
        """
        An enum type to specify which variant of the assembly input is shown.
        """

        class_: typing.ClassVar[java.lang.Class]
        INVISIBLE: typing.Final[AssemblyDualTextField.VisibilityMode]
        """
        Hide both variants. Nothing is shown.
        """

        DUAL_VISIBLE: typing.Final[AssemblyDualTextField.VisibilityMode]
        """
        Show the dual-box linked variant, suitable when the current instruction has operands.
        """

        SINGLE_VISIBLE: typing.Final[AssemblyDualTextField.VisibilityMode]
        """
        Show the single-box unlinked variant, suitable when the current instruction has no
        operands.
        """


        @staticmethod
        def valueOf(name: typing.Union[java.lang.String, str]) -> AssemblyDualTextField.VisibilityMode:
            ...

        @staticmethod
        def values() -> jpype.JArray[AssemblyDualTextField.VisibilityMode]:
            ...


    class AssemblyDualTextFieldDemo(ghidra.GhidraLaunchable):
        """
        A demonstration of the assembly GUI outside of Ghidra
        """

        class_: typing.ClassVar[java.lang.Class]
        DEMO_LANG_ID: typing.Final[ghidra.program.model.lang.LanguageID]
        ADDR_FORMAT: typing.Final = "@%08x:"

        def __init__(self) -> None:
            ...


    class_: typing.ClassVar[java.lang.Class]

    def __init__(self) -> None:
        """
        Construct the assembly text fields
        """

    def addFocusListener(self, listener: java.awt.event.FocusListener) -> None:
        """
        Add a focus listener to the box(es)
        
        
        
        **NOTE:** The listener will not fire when focus passes among the linked boxes of the dual
        variant.
        
        :param java.awt.event.FocusListener listener: the listener
        """

    def addKeyListener(self, listener: java.awt.event.KeyListener) -> None:
        """
        Add a key listener to the box(es)
        
        :param java.awt.event.KeyListener listener: the listener
        """

    def clear(self) -> None:
        """
        Clear all text boxes
        """

    def getAssemblyField(self) -> javax.swing.JTextField:
        """
        For single mode: Get the text field containing the full assembly text
        
        :return: the text field
        :rtype: javax.swing.JTextField
        """

    def getAutocompleter(self) -> docking.widgets.autocomplete.TextComponentAutocompleter[ghidra.app.plugin.core.assembler.completion.AssemblyCompletion]:
        """
        Get a reference to the autocompleter
        
        
        
        This is useful for adding the custom listener needed to detect activation of assembled
        instruction entries.
        
        :return: the autocompleter
        :rtype: docking.widgets.autocomplete.TextComponentAutocompleter[ghidra.app.plugin.core.assembler.completion.AssemblyCompletion]
        """

    def getExhaustButton(self) -> javax.swing.JButton:
        """
        Get the button that toggles bit exhaustion
        
        :return: the button
        :rtype: javax.swing.JButton
        """

    def getMnemonicField(self) -> javax.swing.JTextField:
        """
        For dual mode: Get the text field containing the mnemonic portion of the assembly
        
        :return: the text field
        :rtype: javax.swing.JTextField
        """

    def getOperandsField(self) -> javax.swing.JTextField:
        """
        For dual mode: Get the text field containing the operands portion of the assembly
        
        :return: the text field
        :rtype: javax.swing.JTextField
        """

    def getText(self) -> str:
        """
        Get the full assembly text
        
        :return: the text
        :rtype: str
        """

    def getVisible(self) -> AssemblyDualTextField.VisibilityMode:
        """
        Get the visibility of the text box(es)
        
        
        
        **NOTE:** This method assumes nothing else changes the visibility of the text boxes. If
        anything else does, then it should be sure to maintain a configuration consistent with one of
        the :obj:`VisibilityMode`s.
        
        :return: the current mode
        :rtype: AssemblyDualTextField.VisibilityMode
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

    def setCaretPosition(self, pos: typing.Union[jpype.JInt, int]) -> None:
        """
        Set the caret position of the visible field(s)
        
        :param jpype.JInt or int pos: the position
        """

    def setExisting(self, existing: ghidra.program.model.listing.Instruction) -> None:
        """
        Set the "existing" instruction used for ordering proposed instructions by "most similar"
        
        :param ghidra.program.model.listing.Instruction existing: the existing instruction
        """

    def setFont(self, font: java.awt.Font) -> None:
        """
        Set the font for all text fields
        
        :param java.awt.Font font: the new font
        """

    def setText(self, text: typing.Union[java.lang.String, str]) -> None:
        """
        Set the text of the visible field(s)
        
        :param java.lang.String or str text: the text
        """

    def setVisible(self, visibility: AssemblyDualTextField.VisibilityMode) -> None:
        """
        Set the visibility of the text box(es)
        
        :param AssemblyDualTextField.VisibilityMode visibility: the :obj:`VisibilityMode` to set.
        """

    @property
    def assemblyField(self) -> javax.swing.JTextField:
        ...

    @property
    def visible(self) -> AssemblyDualTextField.VisibilityMode:
        ...

    @visible.setter
    def visible(self, value: AssemblyDualTextField.VisibilityMode):
        ...

    @property
    def exhaustButton(self) -> javax.swing.JButton:
        ...

    @property
    def autocompleter(self) -> docking.widgets.autocomplete.TextComponentAutocompleter[ghidra.app.plugin.core.assembler.completion.AssemblyCompletion]:
        ...

    @property
    def text(self) -> java.lang.String:
        ...

    @text.setter
    def text(self, value: java.lang.String):
        ...

    @property
    def mnemonicField(self) -> javax.swing.JTextField:
        ...

    @property
    def operandsField(self) -> javax.swing.JTextField:
        ...


class PatchInstructionAction(AbstractPatchAction):
    """
    A context menu action to assemble an instruction at the current address
    """

    @typing.type_check_only
    class ListenerForAccept(docking.widgets.autocomplete.AutocompletionListener[ghidra.app.plugin.core.assembler.completion.AssemblyCompletion]):
        """
        A listener for activation of a completion item
        
        
        
        The :meth:`AbstractPatchAction.accept() <AbstractPatchAction.accept>` method does not suffice for this action, since one
        of the suggested byte sequences must be selected, as presented by the completer. Thus, we'll
        nop that method, and instead call our own acceptance logic from here.
        """

        class_: typing.ClassVar[java.lang.Class]


    class_: typing.ClassVar[java.lang.Class]

    @typing.overload
    def __init__(self, owner: ghidra.framework.plugintool.Plugin) -> None:
        ...

    @typing.overload
    def __init__(self, owner: ghidra.framework.plugintool.Plugin, name: typing.Union[java.lang.String, str]) -> None:
        ...

    def accept(self, ins: ghidra.app.plugin.core.assembler.completion.InstructionAssemblyCompletion) -> None:
        """
        Accept the given instruction selected by the user
        
        :param ghidra.app.plugin.core.assembler.completion.InstructionAssemblyCompletion ins: the selected instruction from the completion list
        """


class PatchDataAction(AbstractPatchAction):
    """
    A context menu action to patch data at the current address
    """

    class_: typing.ClassVar[java.lang.Class]

    @typing.overload
    def __init__(self, owner: ghidra.framework.plugintool.Plugin) -> None:
        ...

    @typing.overload
    def __init__(self, owner: ghidra.framework.plugintool.Plugin, name: typing.Union[java.lang.String, str]) -> None:
        ...


class PatchAssemblyCommand(AbstractPatchAssemblyCommand[ghidra.program.model.listing.Program]):

    class_: typing.ClassVar[java.lang.Class]

    @typing.overload
    def __init__(self, asm: ghidra.app.plugin.assembler.Assembler, lines: java.util.List[java.lang.String], entry: ghidra.program.model.address.Address, initialContext: ghidra.program.model.lang.RegisterValue) -> None:
        ...

    @typing.overload
    def __init__(self, asm: ghidra.app.plugin.assembler.Assembler, string: typing.Union[java.lang.String, str], entry: ghidra.program.model.address.Address, initialContext: ghidra.program.model.lang.RegisterValue) -> None:
        ...



__all__ = ["AbstractAssemblePatchDialog", "AbstractPatchAssemblyCommand", "AbstractPatchAction", "AssemblePatchAction", "AssemblePatchDialog", "AssemblerPlugin", "AssemblyDualTextField", "PatchInstructionAction", "PatchDataAction", "PatchAssemblyCommand"]
