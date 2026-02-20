from nlr.routee.compass.compass_app import CompassApp
from nlr.bambam.bambam_py_api import BambamAppWrapper

class BambamRunner(CompassApp):
    """
    Python app interface for loading and running BAMBAM
    """
    @classmethod
    def get_constructor(cls) -> BambamAppWrapper:
        """Override to use bambam's wrapper with extended builders"""
        return BambamAppWrapper