<?php

namespace App;

class Noise14
{
    private function compute(): int
    {
        return 14;
    }

    public function process(): int
    {
        return $this->compute() + $this->process();
    }
}
