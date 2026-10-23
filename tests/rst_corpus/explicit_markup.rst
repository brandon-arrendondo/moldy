.. note:: Text on the first line
   continues here.

   Second paragraph.

.. warning::
   :class: special

   Indented body.

.. code-block:: python
   :linenos:

   x=1
   if x :  print( x )

.. figure:: picture.png
   :alt: A picture

   The caption is rewrapped like any other paragraph because the figure body is reStructuredText.

.. list-table:: Title
   :header-rows: 1

   * - Key
     - Value
   * - a
     - b

.. mydirective:: argument
   :option:   value

   Content  kept   as written.

.. versionadded:: 1.2

   Body text.

.. _target: https://example.com

.. __: https://anonymous.example.com

.. [1] A footnote that is long enough to wrap onto a second line when it is formatted by moldy.

.. [#auto] An auto-numbered footnote.

.. [CIT2002] A citation.

.. |sub| replace:: replacement text

.. |img| image:: picture.png
   :alt: image
